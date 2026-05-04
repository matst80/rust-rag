use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::api::AppState;
use crate::config::ManagerConfig;
use crate::db::{
    ItemRecord, ListItemsRequest, MessageQuery, MessageRecord, MessageSenderKind, MessageUpdate,
    NewMessage, SortOrder,
};

const MANAGER_SENDER: &str = "manager";
const MAX_RECENT_MESSAGES: usize = 40;
const MAX_RECALL: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Cron,
    ManagerChannel,
    Mention,
}

impl Trigger {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cron => "cron",
            Self::ManagerChannel => "manager_channel",
            Self::Mention => "mention",
        }
    }
}

pub async fn run_manager_worker(
    state: AppState,
    cfg: ManagerConfig,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    let base_url = cfg
        .base_url
        .clone()
        .or_else(|| state.openai_chat.base_url.clone());
    if base_url.is_none() {
        warn!("manager worker: disabled (no base URL — set RAG_MANAGER_API_BASE_URL or RAG_OPENAI_API_BASE_URL)");
        return;
    }
    let model = match cfg
        .model
        .clone()
        .or_else(|| state.openai_chat.default_model.clone())
    {
        Some(m) => m,
        None => {
            warn!("manager worker: disabled (no model configured)");
            return;
        }
    };

    info!(
        "manager worker: starting — channel={} mention={} interval={}s model={}",
        cfg.channel, cfg.mention, cfg.interval_secs, model
    );

    // Initial presence touch + startup ping so users see manager as active.
    state.presence.touch(&cfg.channel, MANAGER_SENDER, "agent");
    if let Err(err) = post_startup_ping(&state, &cfg).await {
        warn!("manager worker: startup ping failed: {err}");
    }

    let mut last_seen = current_millis();
    let mut last_cron_at = last_seen;
    let mut ticker = interval(Duration::from_secs(cfg.interval_secs.max(10)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Presence heartbeat ~ every 10s (window is 30s).
    let mut presence_ticker = interval(Duration::from_secs(10));
    presence_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let notify = state.message_notify.clone();
        let notified = notify.notified();
        tokio::pin!(notified);

        tokio::select! {
            _ = ticker.tick() => {
                state.presence.touch(&cfg.channel, MANAGER_SENDER, "agent");
                // Cron path: scan any messages we missed first.
                if let Err(err) = handle_new_messages(&state, &cfg, &model, &mut last_seen).await {
                    error!("manager worker: cron pre-scan error: {err}");
                }
                // Skip cron iteration if no new activity since the last cron tick.
                // last_seen advances on every observed message (pre-scan or notify path).
                if last_seen > last_cron_at {
                    if let Err(err) = run_iteration(&state, &cfg, &model, Trigger::Cron, None, &mut last_seen).await {
                        error!("manager worker: cron iteration error: {err}");
                    }
                } else {
                    debug!("manager worker: cron tick skipped (no new messages since last cron)");
                }
                last_cron_at = last_seen.max(current_millis());
            }
            _ = presence_ticker.tick() => {
                state.presence.touch(&cfg.channel, MANAGER_SENDER, "agent");
            }
            _ = &mut notified => {
                state.presence.touch(&cfg.channel, MANAGER_SENDER, "agent");
                if let Err(err) = handle_new_messages(&state, &cfg, &model, &mut last_seen).await {
                    error!("manager worker: notify iteration error: {err}");
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("manager worker: shutdown signal received, exiting loop");
                    break;
                }
            }
        }
    }
}

async fn post_startup_ping(state: &AppState, cfg: &ManagerConfig) -> Result<()> {
    let now = current_millis();
    let new_msg = NewMessage {
        id: uuid::Uuid::now_v7().to_string(),
        channel: cfg.channel.clone(),
        sender: MANAGER_SENDER.to_owned(),
        sender_kind: MessageSenderKind::System,
        text: "manager online".to_owned(),
        kind: "text".to_owned(),
        metadata: serde_json::json!({"event": "startup"}),
        created_at: now,
    };
    let store = state.messages.clone();
    tokio::task::spawn_blocking(move || store.send_message(new_msg)).await??;
    state.message_notify.notify_waiters();
    Ok(())
}

async fn handle_new_messages(
    state: &AppState,
    cfg: &ManagerConfig,
    model: &str,
    last_seen: &mut i64,
) -> Result<()> {
    let messages = state.messages.clone();
    let since = *last_seen;
    let new_messages = tokio::task::spawn_blocking(move || -> Result<Vec<MessageRecord>> {
        let (rows, _) = messages.list_messages(MessageQuery {
            min_created_at: Some(since + 1),
            limit: Some(50),
            sort_order: SortOrder::Asc,
            ..Default::default()
        })?;
        Ok(rows)
    })
    .await??;

    if new_messages.is_empty() {
        return Ok(());
    }

    for msg in &new_messages {
        if msg.created_at > *last_seen {
            *last_seen = msg.created_at;
        }
    }

    // Skip messages the manager itself sent.
    let triggers: Vec<(Trigger, &MessageRecord)> = new_messages
        .iter()
        .filter(|m| m.sender != MANAGER_SENDER)
        .filter_map(|m| classify_trigger(m, cfg).map(|t| (t, m)))
        .collect();

    if triggers.is_empty() {
        return Ok(());
    }

    // Coalesce: one iteration per channel that triggered.
    let mut seen_channels: std::collections::HashSet<String> = Default::default();
    for (trigger, msg) in triggers {
        if !seen_channels.insert(msg.channel.clone()) {
            continue;
        }
        if let Err(err) =
            run_iteration(state, cfg, model, trigger, Some(msg.clone()), last_seen).await
        {
            error!("manager worker: trigger iteration error: {err}");
        }
    }
    Ok(())
}

fn classify_trigger(msg: &MessageRecord, cfg: &ManagerConfig) -> Option<Trigger> {
    if msg.channel == cfg.channel {
        return Some(Trigger::ManagerChannel);
    }
    if msg.text.contains(&cfg.mention) {
        return Some(Trigger::Mention);
    }
    None
}

async fn run_iteration(
    state: &AppState,
    cfg: &ManagerConfig,
    model: &str,
    trigger: Trigger,
    triggering_message: Option<MessageRecord>,
    last_seen: &mut i64,
) -> Result<()> {
    let target_channel = triggering_message
        .as_ref()
        .map(|m| m.channel.clone())
        .unwrap_or_else(|| cfg.channel.clone());

    let recent = recent_channel_messages(state, &target_channel, MAX_RECENT_MESSAGES).await?;
    let memory = recall_items(state, cfg, None, None, MAX_RECALL).await?;

    let mut thinking_id: Option<String> = None;
    // Manager-channel and mention triggers post a visible thinking placeholder
    // so users can see the manager is working. Cron stays silent unless action.
    if matches!(trigger, Trigger::ManagerChannel | Trigger::Mention) {
        match start_thinking_message(state, &target_channel, "thinking…").await {
            Ok(id) => thinking_id = Some(id),
            Err(err) => warn!("manager: failed to post thinking placeholder: {err}"),
        }
    }

    let mut chat_messages = vec![json!({
        "role": "system",
        "content": cfg.system_prompt,
    })];

    let trigger_payload = json!({
        "trigger": trigger.as_str(),
        "channel": target_channel,
        "now_ms": current_millis(),
        "triggering_message": triggering_message.as_ref().map(message_to_json),
        "recent_messages": recent.iter().map(message_to_json).collect::<Vec<_>>(),
        "memory_recall": memory.iter().map(item_to_memory_json).collect::<Vec<_>>(),
        "manager_channel": cfg.channel,
        "mention": cfg.mention,
    });

    chat_messages.push(json!({
        "role": "user",
        "content": format!(
            "Trigger context (JSON):\n{}\n\nDecide what (if anything) to do. Use tools to act. If no action is warranted, do not call any tool and produce no output.",
            serde_json::to_string_pretty(&trigger_payload).unwrap_or_default()
        ),
    }));

    let mut accumulated = AccumulatedText::default();
    for iter in 0..cfg.max_iterations {
        debug!(iteration = iter, "manager worker: chat call");

        // Lazy-create placeholder for cron triggers as soon as we begin a chat call.
        if thinking_id.is_none() {
            if let Ok(id) = start_thinking_message(state, &target_channel, "…").await {
                thinking_id = Some(id);
            }
        }

        let stream_result =
            stream_chat(state, model, &chat_messages, thinking_id.as_deref(), &mut accumulated)
                .await?;

        chat_messages.push(serde_json::to_value(&stream_result.assistant_message)?);

        if !stream_result.tool_calls.is_empty() {
            // Append a small marker so users see what's happening in-line.
            let names: Vec<String> = stream_result
                .tool_calls
                .iter()
                .map(|c| c.function.name.clone())
                .collect();
            accumulated
                .extra
                .push_str(&format!("\n\n_→ calling: {}_\n", names.join(", ")));
            if let Some(id) = thinking_id.as_deref() {
                let _ = update_thinking(state, id, &accumulated.render(), true).await;
            }
            for call in &stream_result.tool_calls {
                let result = execute_tool(state, cfg, last_seen, &call.function).await;
                let content = match result {
                    Ok(value) => value,
                    Err(err) => json!({ "error": err.to_string() }).to_string(),
                };
                chat_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call.id,
                    "name": call.function.name,
                    "content": content,
                }));
            }
            // Reset content/reasoning for next pass; keep extra (tool markers) so users see history.
            accumulated.reasoning.clear();
            accumulated.content.clear();
            continue;
        }

        // No tool calls — final assistant turn.
        break;
    }

    if let Some(id) = thinking_id {
        let final_body = accumulated.render_final();
        let _ = update_thinking(
            state,
            &id,
            if final_body.trim().is_empty() {
                "(done)"
            } else {
                &final_body
            },
            false,
        )
        .await;
    }

    Ok(())
}

#[derive(Debug, Default)]
struct AccumulatedText {
    reasoning: String,
    content: String,
    /// Inline markers like tool-call notices that persist across iterations.
    extra: String,
}

impl AccumulatedText {
    fn render(&self) -> String {
        let mut out = String::new();
        if !self.reasoning.trim().is_empty() {
            out.push_str("> 💭 ");
            out.push_str(&self.reasoning.replace('\n', "\n> "));
            out.push_str("\n\n");
        }
        if !self.content.trim().is_empty() {
            out.push_str(&self.content);
        }
        if !self.extra.is_empty() {
            out.push_str(&self.extra);
        }
        out
    }

    fn render_final(&self) -> String {
        // Final view: prefer content. Drop reasoning to keep finished message clean,
        // unless there's no content at all.
        if !self.content.trim().is_empty() {
            let mut out = self.content.clone();
            if !self.extra.is_empty() {
                out.push_str(&self.extra);
            }
            out
        } else {
            self.render()
        }
    }
}

struct StreamResult {
    assistant_message: Value,
    tool_calls: Vec<ToolCall>,
}

async fn stream_chat(
    state: &AppState,
    model: &str,
    messages: &[Value],
    placeholder_id: Option<&str>,
    accumulated: &mut AccumulatedText,
) -> Result<StreamResult> {
    let openai = state.openai_chat.clone();
    let manager_cfg = state.manager_runtime.clone();
    let base_url = manager_cfg
        .as_ref()
        .and_then(|c| c.base_url.clone())
        .or_else(|| openai.base_url.clone())
        .ok_or_else(|| anyhow!("manager: no base_url configured"))?;
    let base_url = base_url.trim_end_matches('/').to_owned();
    let api_key = manager_cfg
        .as_ref()
        .and_then(|c| c.api_key.clone())
        .or_else(|| openai.api_key.clone());

    let payload = json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "tools": tool_definitions(),
        "tool_choice": "auto",
        "parallel_tool_calls": false,
    });

    let mut req = state
        .http_client
        .post(format!("{base_url}/chat/completions"))
        .json(&payload);
    if let Some(key) = api_key.as_deref() {
        req = req.bearer_auth(key);
    }

    let response = req.send().await?.error_for_status()?;
    let mut stream = response.bytes_stream();

    let mut buffer = SseBuffer::default();
    let mut tool_acc: HashMap<usize, PartialToolCall> = HashMap::new();
    let mut last_pushed_len = accumulated.render().len();
    let mut last_flush = std::time::Instant::now();
    let flush_interval = std::time::Duration::from_millis(400);
    let flush_chars: usize = 60;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let text = std::str::from_utf8(&chunk).unwrap_or_default();
        for event in buffer.push(text) {
            if event == "[DONE]" {
                continue;
            }
            let parsed: ChatCompletionChunk = match serde_json::from_str(&event) {
                Ok(p) => p,
                Err(err) => {
                    warn!("manager: failed to parse chunk: {err}; raw={event}");
                    continue;
                }
            };
            for choice in parsed.choices {
                if let Some(content) = choice.delta.content {
                    accumulated.content.push_str(&content);
                }
                if let Some(reasoning) = choice.delta.reasoning_content {
                    accumulated.reasoning.push_str(&reasoning);
                }
                if let Some(deltas) = choice.delta.tool_calls {
                    for d in deltas {
                        let entry = tool_acc.entry(d.index).or_default();
                        if let Some(id) = d.id {
                            entry.id = id;
                        }
                        if let Some(kind) = d.kind {
                            entry.kind = kind;
                        }
                        if let Some(func) = d.function {
                            if let Some(name) = func.name {
                                entry.name.push_str(&name);
                            }
                            if let Some(args) = func.arguments {
                                entry.arguments.push_str(&args);
                            }
                        }
                    }
                }
            }
        }

        // Throttled flush to placeholder.
        let rendered = accumulated.render();
        let grew_enough = rendered.len() >= last_pushed_len + flush_chars;
        let waited_enough = last_flush.elapsed() >= flush_interval;
        if let Some(id) = placeholder_id {
            if grew_enough || waited_enough {
                let _ = update_thinking(state, id, &rendered, true).await;
                last_pushed_len = rendered.len();
                last_flush = std::time::Instant::now();
            }
        }
    }

    // Final flush.
    if let Some(id) = placeholder_id {
        let _ = update_thinking(state, id, &accumulated.render(), true).await;
    }

    let mut tool_calls: Vec<ToolCall> = tool_acc
        .into_iter()
        .filter(|(_, v)| !v.id.is_empty() && !v.name.is_empty())
        .map(|(idx, v)| (idx, v))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(_, v)| ToolCall {
            id: v.id,
            kind: if v.kind.is_empty() {
                "function".to_owned()
            } else {
                v.kind
            },
            function: ToolCallFunction {
                name: v.name,
                arguments: v.arguments,
            },
        })
        .collect();
    tool_calls.sort_by(|a, b| a.id.cmp(&b.id));

    let assistant_message = json!({
        "role": "assistant",
        "content": if accumulated.content.is_empty() { Value::Null } else { Value::String(accumulated.content.clone()) },
        "tool_calls": if tool_calls.is_empty() { Value::Null } else { serde_json::to_value(&tool_calls)? },
    });

    Ok(StreamResult {
        assistant_message,
        tool_calls,
    })
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: String,
    kind: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Default)]
struct SseBuffer {
    buf: String,
}

impl SseBuffer {
    fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buf.push_str(&chunk.replace("\r\n", "\n"));
        let mut out = Vec::new();
        while let Some(pos) = self.buf.find("\n\n") {
            let raw = self.buf[..pos].to_owned();
            self.buf.drain(..pos + 2);
            let data: String = raw
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                out.push(data);
            }
        }
        out
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<ChatCompletionChoiceChunk>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoiceChunk {
    delta: ChatCompletionDelta,
    #[serde(default)]
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ChatCompletionDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default, alias = "reasoning")]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    function: Option<ToolCallFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ToolCallFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

async fn start_thinking_message(
    state: &AppState,
    channel: &str,
    initial_text: &str,
) -> Result<String> {
    let now = current_millis();
    let id = uuid::Uuid::now_v7().to_string();
    let new_msg = NewMessage {
        id: id.clone(),
        channel: channel.to_owned(),
        sender: MANAGER_SENDER.to_owned(),
        sender_kind: MessageSenderKind::Agent,
        text: initial_text.to_owned(),
        kind: "agent_chunk".to_owned(),
        metadata: json!({"thinking": true, "manager": true}),
        created_at: now,
    };
    let store = state.messages.clone();
    tokio::task::spawn_blocking(move || store.send_message(new_msg)).await??;
    state.message_notify.notify_waiters();
    Ok(id)
}

async fn update_thinking(
    state: &AppState,
    id: &str,
    text: &str,
    thinking: bool,
) -> Result<()> {
    let store = state.messages.clone();
    let id_owned = id.to_owned();
    let text_owned = text.to_owned();
    let metadata = json!({"thinking": thinking, "manager": true});
    let now = current_millis();
    tokio::task::spawn_blocking(move || -> Result<()> {
        store.update_message(
            &id_owned,
            MessageUpdate {
                text: Some(text_owned),
                metadata: Some(metadata),
                append_text: false,
            },
            now,
        )?;
        Ok(())
    })
    .await??;
    state.message_notify.notify_waiters();
    Ok(())
}


#[derive(Debug, Deserialize, serde::Serialize, Clone)]
struct ToolCall {
    id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    kind: String,
    function: ToolCallFunction,
}

fn default_tool_type() -> String {
    "function".to_owned()
}

#[derive(Debug, Deserialize, serde::Serialize, Clone)]
struct ToolCallFunction {
    name: String,
    arguments: String,
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "post_message",
                "description": "Post a message to a rust-rag channel. Use for human-visible status, agent-to-agent collaboration handoffs, or summaries. NOT used for ACP agent control — use the acp_* tools for that. sender_kind defaults to 'system'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "channel": {"type": "string"},
                        "text": {"type": "string"},
                        "kind": {"type": "string", "default": "text"},
                        "metadata": {"type": "object"},
                        "sender_kind": {"type": "string", "enum": ["human", "agent", "system"]}
                    },
                    "required": ["channel", "text"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_channel",
                "description": "Fetch the most recent messages from a channel.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "channel": {"type": "string"},
                        "limit": {"type": "integer", "default": 30, "minimum": 1, "maximum": 200}
                    },
                    "required": ["channel"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_channels",
                "description": "List all channels with their message counts and last activity timestamp.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "remember",
                "description": "Persist a durable note in manager_memory. kind: summary|note|task|observation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "kind": {"type": "string"},
                        "content": {"type": "string"},
                        "metadata": {"type": "object"}
                    },
                    "required": ["kind", "content"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "recall",
                "description": "Search manager memory (RAG namespace) for prior notes. With `query` it does semantic search; without it just lists by kind. Returns most-relevant first.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "kind": {"type": "string", "description": "Filter by metadata.kind (summary|note|task|observation)."},
                        "query": {"type": "string", "description": "Natural-language search; uses hybrid vector+BM25."},
                        "limit": {"type": "integer", "default": 20, "minimum": 1, "maximum": 100}
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "forget",
                "description": "Delete a memory entry by id.",
                "parameters": {
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_agents",
                "description": "List all active agents (sender_kind=agent) across channels with last activity. Useful before routing or assigning work.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "channel_summary",
                "description": "Get quick stats for a channel: sender breakdown, last activity, and last N message previews. Cheap (no LLM).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "channel": {"type": "string"},
                        "preview_count": {"type": "integer", "default": 5, "minimum": 1, "maximum": 30}
                    },
                    "required": ["channel"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "assign_task",
                "description": "Create a durable task in manager_memory (kind=task) and post a notification to the target channel. Status starts as 'pending'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "description": {"type": "string"},
                        "assigned_to": {"type": "string", "description": "Agent or user name (without @)."},
                        "channel": {"type": "string", "description": "Channel where the assignee will see the work."}
                    },
                    "required": ["title", "assigned_to", "channel"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_tasks",
                "description": "List tasks (manager_memory kind=task) with optional filters.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "status": {"type": "string", "enum": ["pending", "in_progress", "done", "blocked"]},
                        "assigned_to": {"type": "string"},
                        "channel": {"type": "string"},
                        "limit": {"type": "integer", "default": 50, "minimum": 1, "maximum": 200}
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "update_task",
                "description": "Update a task's status and optional notes. Use the memory id returned by assign_task or list_tasks.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "status": {"type": "string", "enum": ["pending", "in_progress", "done", "blocked"]},
                        "note": {"type": "string"}
                    },
                    "required": ["id", "status"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "promote_memory",
                "description": "Move a memory entry into a different RAG namespace (e.g. promote a private observation into shared knowledge). Provide the item id and the new source_id.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "source_id": {"type": "string"}
                    },
                    "required": ["id", "source_id"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_rag",
                "description": "Search the RAG knowledge base for relevant entries to inject as context.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "top_k": {"type": "integer", "default": 5, "minimum": 1, "maximum": 25},
                        "source_id": {"type": "string"}
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }
            }
        }),
        // --- ACP WebSocket tools (telegram-acp surface) ---
        json!({
            "type": "function",
            "function": {
                "name": "acp_list_sessions",
                "description": "Ask telegram-acp to send a fresh ListSessions response over WS. Inspect the result via acp_recent_events with kind=ListSessions.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_spawn",
                "description": "Spawn a new headless ACP session via WS. Returns immediately; new session id arrives as a SessionStarted event.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "project_path": {"type": "string"},
                        "agent_command": {"type": "string"},
                        "metadata": {"type": "object"}
                    },
                    "required": ["project_path"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_send_prompt",
                "description": "Send a prompt to an existing ACP session over WS.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "text": {"type": "string"},
                        "attachments": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["session_id", "text"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_cancel",
                "description": "Cancel the currently running prompt on an ACP session.",
                "parameters": {
                    "type": "object",
                    "properties": {"session_id": {"type": "string"}},
                    "required": ["session_id"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_end_session",
                "description": "Gracefully terminate an ACP session. Provide session_id (preferred) or thread_id fallback.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "thread_id": {"type": "integer"}
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_set_permission_mode",
                "description": "Switch a session between auto and manual tool-call approval.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "mode": {"type": "string", "enum": ["auto", "manual"]}
                    },
                    "required": ["session_id", "mode"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_set_config",
                "description": "Set a per-session config option on an ACP agent.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "key": {"type": "string"},
                        "value": {"type": "string", "description": "JSON-encoded value. Server parses."}
                    },
                    "required": ["session_id", "key", "value"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_permission_respond",
                "description": "Reply to an outstanding PermissionRequest. decision ∈ allow_once|allow_always|deny|deny_always.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "request_id": {"type": "string"},
                        "decision": {"type": "string", "enum": ["allow_once", "allow_always", "deny", "deny_always"]}
                    },
                    "required": ["request_id", "decision"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_recent_events",
                "description": "Read recent ACP WS events from the in-process ring buffer. Filter by session_id, since_local_seq, or kinds. Manager process buffers up to ~200 events per session.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "since_local_seq": {"type": "integer"},
                        "kinds": {"type": "array", "items": {"type": "string"}},
                        "limit": {"type": "integer", "default": 50, "minimum": 1, "maximum": 500}
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_pending_permissions",
                "description": "List outstanding PermissionRequest events awaiting a decision.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "acp_get_snapshot",
                "description": "Return the most recent Snapshot event the WS client has seen (or null if none yet).",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
    ]
}

async fn execute_tool(
    state: &AppState,
    cfg: &ManagerConfig,
    last_seen: &mut i64,
    function: &ToolCallFunction,
) -> Result<String> {
    match function.name.as_str() {
        "post_message" => tool_post_message(state, cfg, last_seen, &function.arguments).await,
        "read_channel" => tool_read_channel(state, &function.arguments).await,
        "list_channels" => tool_list_channels(state).await,
        "remember" => tool_remember(state, cfg, &function.arguments).await,
        "recall" => tool_recall(state, cfg, &function.arguments).await,
        "forget" => tool_forget(state, &function.arguments).await,
        "search_rag" => tool_search_rag(state, &function.arguments).await,
        "list_agents" => tool_list_agents(state).await,
        "channel_summary" => tool_channel_summary(state, &function.arguments).await,
        "assign_task" => tool_assign_task(state, cfg, last_seen, &function.arguments).await,
        "list_tasks" => tool_list_tasks(state, cfg, &function.arguments).await,
        "update_task" => tool_update_task(state, &function.arguments).await,
        "promote_memory" => tool_promote_memory(state, &function.arguments).await,
        "acp_list_sessions" => tool_acp_simple(state, "list_sessions", json!({})).await,
        "acp_spawn" => tool_acp_passthrough(state, "spawn_session", &function.arguments).await,
        "acp_send_prompt" => tool_acp_passthrough(state, "send_prompt", &function.arguments).await,
        "acp_cancel" => tool_acp_passthrough(state, "cancel", &function.arguments).await,
        "acp_end_session" => tool_acp_passthrough(state, "end_session", &function.arguments).await,
        "acp_set_permission_mode" => {
            tool_acp_passthrough(state, "set_permission_mode", &function.arguments).await
        }
        "acp_set_config" => tool_acp_passthrough(state, "set_config_option", &function.arguments).await,
        "acp_permission_respond" => tool_acp_permission_respond(state, &function.arguments).await,
        "acp_recent_events" => tool_acp_recent_events(state, &function.arguments).await,
        "acp_pending_permissions" => tool_acp_pending_permissions(state).await,
        "acp_get_snapshot" => tool_acp_get_snapshot(state).await,
        other => Err(anyhow!("unsupported tool {other}")),
    }
}

fn require_acp(state: &AppState) -> Result<&crate::acp_ws::AcpWsHandle> {
    state
        .acp_ws
        .as_ref()
        .ok_or_else(|| anyhow!("ACP WS client not configured (set RAG_ACP_WS_URL)"))
}

async fn tool_acp_simple(state: &AppState, variant: &str, payload: Value) -> Result<String> {
    let handle = require_acp(state)?;
    handle.command(variant, payload)?;
    Ok(json!({"ok": true, "sent": variant}).to_string())
}

async fn tool_acp_passthrough(state: &AppState, variant: &str, args: &str) -> Result<String> {
    let handle = require_acp(state)?;
    let payload: Value = if args.trim().is_empty() {
        Value::Object(Default::default())
    } else {
        serde_json::from_str(args)
            .map_err(|err| anyhow!("invalid JSON args for {variant}: {err}"))?
    };
    handle.command(variant, payload)?;
    Ok(json!({"ok": true, "sent": variant}).to_string())
}

async fn tool_acp_permission_respond(state: &AppState, args: &str) -> Result<String> {
    let handle = require_acp(state)?;
    let payload: Value = serde_json::from_str(args)
        .map_err(|err| anyhow!("invalid JSON args for PermissionResponse: {err}"))?;
    let request_id = payload
        .get("request_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("request_id required"))?
        .to_owned();
    handle.command("permission_response", payload.clone())?;
    crate::acp_ws::mark_permission_resolved(handle, &request_id).await;
    Ok(json!({"ok": true, "request_id": request_id}).to_string())
}

#[derive(Debug, Deserialize, Default)]
struct AcpRecentEventsArgs {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    since_local_seq: Option<u64>,
    #[serde(default)]
    kinds: Option<Vec<String>>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn tool_acp_recent_events(state: &AppState, args: &str) -> Result<String> {
    let handle = require_acp(state)?;
    let parsed: AcpRecentEventsArgs = if args.trim().is_empty() {
        AcpRecentEventsArgs::default()
    } else {
        serde_json::from_str(args)
            .map_err(|err| anyhow!("invalid JSON args for acp_recent_events: {err}"))?
    };
    let events = handle
        .recent_events(
            parsed.session_id.as_deref(),
            parsed.since_local_seq,
            parsed.kinds.as_deref(),
            parsed.limit,
        )
        .await;
    Ok(serde_json::to_string(&events)?)
}

async fn tool_acp_pending_permissions(state: &AppState) -> Result<String> {
    let handle = require_acp(state)?;
    let events = handle.pending_permissions().await;
    Ok(serde_json::to_string(&events)?)
}

async fn tool_acp_get_snapshot(state: &AppState) -> Result<String> {
    let handle = require_acp(state)?;
    let snap = handle.latest_snapshot().await;
    Ok(serde_json::to_string(&snap)?)
}

async fn tool_list_agents(state: &AppState) -> Result<String> {
    let presence_map = state.presence.list_all();
    let mut agents: Vec<Value> = Vec::new();
    for (channel, entries) in presence_map {
        for entry in entries {
            if entry.kind != "agent" {
                continue;
            }
            agents.push(json!({
                "user": entry.user,
                "channel": channel,
                "last_seen": entry.last_seen,
            }));
        }
    }
    agents.sort_by(|a, b| {
        b["last_seen"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["last_seen"].as_i64().unwrap_or(0))
    });
    Ok(serde_json::to_string(&json!({ "agents": agents }))?)
}

#[derive(Debug, Deserialize)]
struct ChannelSummaryArgs {
    channel: String,
    #[serde(default)]
    preview_count: Option<usize>,
}

async fn tool_channel_summary(state: &AppState, args: &str) -> Result<String> {
    let args: ChannelSummaryArgs =
        serde_json::from_str(args).context("invalid arguments for channel_summary")?;
    let preview_count = args.preview_count.unwrap_or(5).min(30).max(1);
    let messages = recent_channel_messages(state, &args.channel, 100).await?;
    let total = messages.len();
    let mut by_sender: HashMap<String, i64> = HashMap::new();
    let mut by_kind: HashMap<String, i64> = HashMap::new();
    let mut last_activity: i64 = 0;
    for m in &messages {
        *by_sender.entry(m.sender.clone()).or_insert(0) += 1;
        *by_kind.entry(m.kind.clone()).or_insert(0) += 1;
        if m.created_at > last_activity {
            last_activity = m.created_at;
        }
    }
    let preview: Vec<Value> = messages
        .iter()
        .rev()
        .take(preview_count)
        .map(message_to_json)
        .collect();
    let presence = state.presence.list(&args.channel);
    let active_users: Vec<Value> = presence
        .iter()
        .map(|p| {
            json!({
                "user": p.user,
                "kind": p.kind,
                "last_seen": p.last_seen
            })
        })
        .collect();
    Ok(serde_json::to_string(&json!({
        "channel": args.channel,
        "total_recent": total,
        "by_sender": by_sender,
        "by_kind": by_kind,
        "last_activity": last_activity,
        "active_users": active_users,
        "preview": preview,
    }))?)
}

#[derive(Debug, Deserialize)]
struct AssignTaskArgs {
    title: String,
    #[serde(default)]
    description: Option<String>,
    assigned_to: String,
    channel: String,
}

async fn tool_assign_task(
    state: &AppState,
    cfg: &ManagerConfig,
    last_seen: &mut i64,
    args: &str,
) -> Result<String> {
    let args: AssignTaskArgs =
        serde_json::from_str(args).context("invalid arguments for assign_task")?;
    let body = match args.description.as_deref() {
        Some(desc) if !desc.trim().is_empty() => {
            format!("{}\n\n{}", args.title, desc)
        }
        _ => args.title.clone(),
    };
    let id = uuid::Uuid::now_v7().to_string();
    let metadata = json!({
        "kind": "task",
        "manager": true,
        "assigned_to": args.assigned_to,
        "channel": args.channel,
        "status": "pending",
        "title": args.title,
    });
    let item = ItemRecord {
        id: id.clone(),
        text: body.clone(),
        metadata,
        source_id: cfg.memory_source_id.clone(),
        created_at: current_millis(),
    };
    let embedder = state
        .embedder
        .get_ready()
        .map_err(|e| anyhow!(e.to_string()))?;
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let embedding = embedder.embed(&item.text)?;
        store.upsert_item(item, &embedding)?;
        Ok(())
    })
    .await??;

    // Notify the channel so the assignee sees it.
    let notify_text = format!(
        "@{} task assigned: {} (id: {})",
        args.assigned_to, args.title, id
    );
    let now = current_millis();
    let new_msg = NewMessage {
        id: uuid::Uuid::now_v7().to_string(),
        channel: args.channel.clone(),
        sender: MANAGER_SENDER.to_owned(),
        sender_kind: MessageSenderKind::System,
        text: notify_text,
        kind: "text".to_owned(),
        metadata: json!({"task_id": id, "assigned_to": args.assigned_to}),
        created_at: now,
    };
    let messages = state.messages.clone();
    let posted =
        tokio::task::spawn_blocking(move || messages.send_message(new_msg)).await??;
    state.message_notify.notify_waiters();
    if posted.created_at > *last_seen {
        *last_seen = posted.created_at;
    }

    Ok(json!({
        "id": id,
        "assigned_to": args.assigned_to,
        "channel": args.channel,
        "status": "pending"
    })
    .to_string())
}

#[derive(Debug, Deserialize)]
struct ListTasksArgs {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    assigned_to: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn tool_list_tasks(state: &AppState, cfg: &ManagerConfig, args: &str) -> Result<String> {
    let args: ListTasksArgs =
        serde_json::from_str(args).context("invalid arguments for list_tasks")?;
    let limit = args.limit.unwrap_or(50);
    let mut metadata_filter = HashMap::new();
    metadata_filter.insert("kind".to_owned(), "task".to_owned());
    if let Some(status) = args.status.as_deref() {
        metadata_filter.insert("status".to_owned(), status.to_owned());
    }
    if let Some(assigned) = args.assigned_to.as_deref() {
        metadata_filter.insert("assigned_to".to_owned(), assigned.to_owned());
    }
    if let Some(channel) = args.channel.as_deref() {
        metadata_filter.insert("channel".to_owned(), channel.to_owned());
    }
    let store = state.store.clone();
    let request = ListItemsRequest {
        source_id: Some(cfg.memory_source_id.clone()),
        limit: Some(limit),
        offset: Some(0),
        sort_order: SortOrder::Desc,
        metadata_filter,
        min_created_at: None,
        max_created_at: None,
    };
    let (items, _) = tokio::task::spawn_blocking(move || store.list_items(request)).await??;
    let tasks: Vec<Value> = items
        .into_iter()
        .map(|m| {
            json!({
                "id": m.id,
                "title": m.metadata.get("title").cloned().unwrap_or(Value::Null),
                "content": truncate(&m.text, 500),
                "metadata": m.metadata,
                "created_at": m.created_at,
            })
        })
        .collect();
    Ok(serde_json::to_string(&json!({ "tasks": tasks }))?)
}

#[derive(Debug, Deserialize)]
struct UpdateTaskArgs {
    id: String,
    status: String,
    #[serde(default)]
    note: Option<String>,
}

async fn tool_update_task(state: &AppState, args: &str) -> Result<String> {
    let args: UpdateTaskArgs =
        serde_json::from_str(args).context("invalid arguments for update_task")?;
    let store = state.store.clone();
    let id = args.id.clone();
    let existing = tokio::task::spawn_blocking(move || store.get_item(&id))
        .await??
        .ok_or_else(|| anyhow!("task {} not found", args.id))?;

    let mut metadata = existing.metadata.clone();
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("status".to_owned(), Value::String(args.status.clone()));
        if let Some(note) = args.note.as_deref() {
            obj.insert("last_note".to_owned(), Value::String(note.to_owned()));
        }
    }
    let new_text = match args.note.as_deref() {
        Some(note) if !note.trim().is_empty() => {
            format!("{}\n\n[{}] {}", existing.text, args.status, note)
        }
        _ => existing.text.clone(),
    };

    let updated = ItemRecord {
        id: existing.id.clone(),
        text: new_text,
        metadata,
        source_id: existing.source_id.clone(),
        created_at: existing.created_at,
    };
    let embedder = state
        .embedder
        .get_ready()
        .map_err(|e| anyhow!(e.to_string()))?;
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let embedding = embedder.embed(&updated.text)?;
        store.upsert_item(updated, &embedding)?;
        Ok(())
    })
    .await??;
    Ok(json!({ "id": args.id, "status": args.status }).to_string())
}

#[derive(Debug, Deserialize)]
struct PostMessageArgs {
    channel: String,
    text: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    sender_kind: Option<String>,
}

async fn tool_post_message(
    state: &AppState,
    _cfg: &ManagerConfig,
    last_seen: &mut i64,
    args: &str,
) -> Result<String> {
    let args: PostMessageArgs =
        serde_json::from_str(args).context("invalid arguments for post_message")?;
    let sender_kind = match args.sender_kind.as_deref() {
        Some("human") => MessageSenderKind::Human,
        Some("agent") => MessageSenderKind::Agent,
        _ => MessageSenderKind::System,
    };
    let now = current_millis();
    let id = uuid::Uuid::now_v7().to_string();
    let new_msg = NewMessage {
        id: id.clone(),
        channel: args.channel.clone(),
        sender: MANAGER_SENDER.to_owned(),
        sender_kind,
        text: args.text,
        kind: args.kind.unwrap_or_else(|| "text".to_owned()),
        metadata: args.metadata.unwrap_or_else(|| json!({})),
        created_at: now,
    };
    let store = state.messages.clone();
    let record =
        tokio::task::spawn_blocking(move || store.send_message(new_msg)).await??;
    state.message_notify.notify_waiters();
    // Advance cursor so we don't re-trigger on our own post.
    if record.created_at > *last_seen {
        *last_seen = record.created_at;
    }
    Ok(json!({
        "id": record.id,
        "channel": record.channel,
        "created_at": record.created_at
    })
    .to_string())
}

#[derive(Debug, Deserialize)]
struct ReadChannelArgs {
    channel: String,
    #[serde(default)]
    limit: Option<usize>,
}

async fn tool_read_channel(state: &AppState, args: &str) -> Result<String> {
    let args: ReadChannelArgs =
        serde_json::from_str(args).context("invalid arguments for read_channel")?;
    let limit = args.limit.unwrap_or(30).min(200);
    let messages = recent_channel_messages(state, &args.channel, limit).await?;
    Ok(serde_json::to_string(&json!({
        "channel": args.channel,
        "messages": messages.iter().map(message_to_json).collect::<Vec<_>>()
    }))?)
}

async fn tool_list_channels(state: &AppState) -> Result<String> {
    let store = state.messages.clone();
    let channels = tokio::task::spawn_blocking(move || store.list_channels()).await??;
    let json_channels: Vec<Value> = channels
        .into_iter()
        .map(|c| {
            json!({
                "channel": c.channel,
                "message_count": c.message_count,
                "last_message_at": c.last_message_at
            })
        })
        .collect();
    Ok(serde_json::to_string(&json!({ "channels": json_channels }))?)
}

#[derive(Debug, Deserialize)]
struct RememberArgs {
    kind: String,
    content: String,
    #[serde(default)]
    metadata: Option<Value>,
}

async fn tool_remember(state: &AppState, cfg: &ManagerConfig, args: &str) -> Result<String> {
    let args: RememberArgs =
        serde_json::from_str(args).context("invalid arguments for remember")?;
    let mut metadata = args.metadata.unwrap_or_else(|| json!({}));
    if !metadata.is_object() {
        metadata = json!({});
    }
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("kind".to_owned(), Value::String(args.kind.clone()));
        obj.insert("manager".to_owned(), Value::Bool(true));
    }
    let id = uuid::Uuid::now_v7().to_string();
    let source_id = cfg.memory_source_id.clone();
    let item = ItemRecord {
        id: id.clone(),
        text: args.content.clone(),
        metadata,
        source_id: source_id.clone(),
        created_at: current_millis(),
    };
    let embedder = state
        .embedder
        .get_ready()
        .map_err(|e| anyhow!(e.to_string()))?;
    let store = state.store.clone();
    let kind = args.kind.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let embedding = embedder.embed(&item.text)?;
        store.upsert_item(item, &embedding)?;
        Ok(())
    })
    .await??;
    Ok(json!({ "id": id, "kind": kind, "source_id": source_id }).to_string())
}

#[derive(Debug, Deserialize)]
struct RecallArgs {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn tool_recall(state: &AppState, cfg: &ManagerConfig, args: &str) -> Result<String> {
    let args: RecallArgs =
        serde_json::from_str(args).context("invalid arguments for recall")?;
    let limit = args.limit.unwrap_or(20).min(100).max(1);
    let memories = recall_items(state, cfg, args.kind.as_deref(), args.query.as_deref(), limit)
        .await?;
    Ok(serde_json::to_string(&json!({
        "memories": memories.iter().map(item_to_memory_json).collect::<Vec<_>>()
    }))?)
}

#[derive(Debug, Deserialize)]
struct ForgetArgs {
    id: String,
}

#[derive(Debug, Deserialize)]
struct PromoteMemoryArgs {
    id: String,
    source_id: String,
}

async fn tool_promote_memory(state: &AppState, args: &str) -> Result<String> {
    let args: PromoteMemoryArgs =
        serde_json::from_str(args).context("invalid arguments for promote_memory")?;
    let store = state.store.clone();
    let id = args.id.clone();
    let existing = tokio::task::spawn_blocking(move || store.get_item(&id))
        .await??
        .ok_or_else(|| anyhow!("memory {} not found", args.id))?;
    let updated = ItemRecord {
        id: existing.id.clone(),
        text: existing.text.clone(),
        metadata: existing.metadata.clone(),
        source_id: args.source_id.clone(),
        created_at: existing.created_at,
    };
    let embedder = state
        .embedder
        .get_ready()
        .map_err(|e| anyhow!(e.to_string()))?;
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let embedding = embedder.embed(&updated.text)?;
        store.upsert_item(updated, &embedding)?;
        Ok(())
    })
    .await??;
    Ok(json!({
        "id": args.id,
        "from_source_id": existing.source_id,
        "to_source_id": args.source_id
    })
    .to_string())
}

async fn tool_forget(state: &AppState, args: &str) -> Result<String> {
    let args: ForgetArgs =
        serde_json::from_str(args).context("invalid arguments for forget")?;
    let store = state.store.clone();
    let id = args.id.clone();
    let id_for_task = id.clone();
    let deleted = tokio::task::spawn_blocking(move || store.delete_item(&id_for_task)).await??;
    Ok(json!({ "id": id, "deleted": deleted }).to_string())
}

#[derive(Debug, Deserialize)]
struct SearchRagArgs {
    query: String,
    #[serde(default)]
    top_k: Option<usize>,
    #[serde(default)]
    source_id: Option<String>,
}

async fn tool_search_rag(state: &AppState, args: &str) -> Result<String> {
    let args: SearchRagArgs =
        serde_json::from_str(args).context("invalid arguments for search_rag")?;
    let embedder = state.embedder.get_ready().map_err(|e| anyhow!(e.to_string()))?;
    let store = state.store.clone();
    let top_k = args.top_k.unwrap_or(5).min(25).max(1);
    let query = args.query.clone();
    let source_id = args.source_id.clone();
    let hits = tokio::task::spawn_blocking(move || -> Result<_> {
        let embedding = embedder.embed(&query)?;
        store.search_hybrid(&query, &embedding, top_k, source_id.as_deref())
    })
    .await??;
    let payload: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            json!({
                "id": h.id,
                "text": truncate(&h.text, 1000),
                "metadata": h.metadata,
                "source_id": h.source_id,
                "distance": h.distance,
            })
        })
        .collect();
    Ok(serde_json::to_string(&json!({ "results": payload }))?)
}

async fn recent_channel_messages(
    state: &AppState,
    channel: &str,
    limit: usize,
) -> Result<Vec<MessageRecord>> {
    let messages = state.messages.clone();
    let channel_owned = channel.to_owned();
    let (rows, _) = tokio::task::spawn_blocking(move || -> Result<_> {
        messages.list_messages(MessageQuery {
            channel: Some(channel_owned),
            limit: Some(limit),
            sort_order: SortOrder::Desc,
            ..Default::default()
        })
    })
    .await??;
    let mut rows = rows;
    rows.reverse();
    Ok(rows)
}

async fn recall_items(
    state: &AppState,
    cfg: &ManagerConfig,
    kind: Option<&str>,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<ItemRecord>> {
    let source_id = cfg.memory_source_id.clone();
    let kind_owned = kind.map(|s| s.to_owned());
    if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
        // Semantic recall.
        let embedder = state
            .embedder
            .get_ready()
            .map_err(|e| anyhow!(e.to_string()))?;
        let store = state.store.clone();
        let q_owned = q.to_owned();
        let source_for_search = source_id.clone();
        let hits = tokio::task::spawn_blocking(move || -> Result<_> {
            let embedding = embedder.embed(&q_owned)?;
            store.search_hybrid(&q_owned, &embedding, limit * 2, Some(&source_for_search))
        })
        .await??;
        // Map hits to ItemRecord-shape via get_item lookups (cheap; small N).
        let store = state.store.clone();
        let ids: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
        let kind_filter = kind_owned.clone();
        let items = tokio::task::spawn_blocking(move || -> Result<Vec<ItemRecord>> {
            let mut out = Vec::new();
            for id in ids {
                if let Some(item) = store.get_item(&id)? {
                    if let Some(k) = kind_filter.as_deref() {
                        if item.metadata.get("kind").and_then(|v| v.as_str()) != Some(k) {
                            continue;
                        }
                    }
                    out.push(item);
                    if out.len() >= limit {
                        break;
                    }
                }
            }
            Ok(out)
        })
        .await??;
        return Ok(items);
    }

    // No query — list by source_id with metadata filter.
    let mut metadata_filter = HashMap::new();
    if let Some(k) = kind_owned {
        metadata_filter.insert("kind".to_owned(), k);
    }
    let store = state.store.clone();
    let request = ListItemsRequest {
        source_id: Some(source_id),
        limit: Some(limit),
        offset: Some(0),
        sort_order: SortOrder::Desc,
        metadata_filter,
        min_created_at: None,
        max_created_at: None,
    };
    let (items, _) = tokio::task::spawn_blocking(move || store.list_items(request)).await??;
    Ok(items)
}

fn message_to_json(m: &MessageRecord) -> Value {
    json!({
        "id": m.id,
        "channel": m.channel,
        "sender": m.sender,
        "sender_kind": m.sender_kind.as_serialized(),
        "text": truncate(&m.text, 2000),
        "kind": m.kind,
        "metadata": m.metadata,
        "created_at": m.created_at,
    })
}

fn item_to_memory_json(m: &ItemRecord) -> Value {
    let kind = m
        .metadata
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("note")
        .to_owned();
    json!({
        "id": m.id,
        "kind": kind,
        "content": truncate(&m.text, 1000),
        "metadata": m.metadata,
        "created_at": m.created_at,
        "source_id": m.source_id,
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("…");
    out
}

fn current_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// Manual public trigger (used by API endpoint).
pub async fn trigger_once(state: Arc<AppState>, cfg: ManagerConfig) -> Result<()> {
    let model = cfg
        .model
        .clone()
        .or_else(|| state.openai_chat.default_model.clone())
        .ok_or_else(|| anyhow!("manager: no model configured"))?;
    let mut last_seen = current_millis();
    run_iteration(&state, &cfg, &model, Trigger::Cron, None, &mut last_seen).await
}

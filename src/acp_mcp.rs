//! Dedicated MCP server for the ACP delegation surface, mounted at `/mcp/acp`.
//!
//! The main `/mcp` server ([`crate::mcp`]) exposes the memory, messaging, and
//! graph tools. Every ACP session/terminal control tool lives here instead, so
//! memory-focused MCP clients don't carry ~24 extra tool schemas in their
//! context. Both endpoints share the same bearer-token and Host-header guards
//! (see `readme_mcp.md`).

use crate::api::{AppState, metadata_schema};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

const ACP_SERVER_NAME: &str = "rust-rag-acp";
const ACP_SERVER_INSTRUCTIONS: &str = "rust-rag ACP delegation surface: drive headless agent daemons over WebSocket (spawn sessions, send prompts, answer permission requests, read terminal output).\
\
FLOW:\
1. `acp_list_instances` to discover daemons; pass `instance` to every tool when several are registered.\
2. `acp_delegate_task` for one-shot spawn-and-prompt, or `acp_spawn_session` + `acp_send_prompt` for long-lived sessions.\
3. Poll `acp_recent_events` for `SessionStarted` / `AssistantMessage` / `ToolCall` events. Use `acp_pending_permissions` + `acp_permission_respond` to unblock manual-approval sessions.\
\
Memory persistence lives on the main rust-rag MCP server at /mcp (`store_entry`, `search_entries`, ...) — this server only controls ACP sessions.";

#[derive(Clone)]
pub struct AcpMcpServer {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl AcpMcpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AcpMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                ACP_SERVER_NAME.to_owned(),
                env!("CARGO_PKG_VERSION").to_owned(),
            ))
            .with_instructions(ACP_SERVER_INSTRUCTIONS.to_owned())
    }
}

#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct AcpInstancesResponse {
    pub instances: Vec<crate::acp_discovery::AcpInstance>,
    pub active: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpSelectInstanceParams {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpSpawnParams {
    pub project_path: String,
    #[serde(default)]
    pub agent_command: Option<String>,
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Option<serde_json::Value>,
    /// Target ACP instance id. Omit when only one is registered.
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpTerminalIdParams {
    pub terminal_id: String,
    /// Target ACP instance id. Omit when only one is registered.
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpSendPromptParams {
    pub session_id: String,
    pub text: String,
    #[serde(default)]
    pub attachments: Option<Vec<String>>,
    /// Target ACP instance id. Omit when only one is registered.
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpSessionIdParams {
    pub session_id: String,
    /// Target ACP instance id. Omit when only one is registered.
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpEndSessionParams {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<i64>,
    /// Target ACP instance id. Omit when only one is registered.
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpSetPermissionModeParams {
    pub session_id: String,
    /// "auto" | "manual"
    pub mode: String,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpPermissionRespondParams {
    pub request_id: String,
    /// allow_once | allow_always | deny | deny_always
    pub decision: String,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct AcpRecentEventsParams {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub since_local_seq: Option<u64>,
    #[serde(default)]
    pub kinds: Option<Vec<String>>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct AcpInstanceParams {
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpCreateTerminalParams {
    #[serde(default)]
    pub session_id: Option<String>,
    pub cwd: String,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    /// Seconds to wait for `terminal_created`. Default 10.
    #[serde(default)]
    pub wait_secs: Option<u64>,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AcpCreateTerminalResponse {
    pub ok: bool,
    pub terminal_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpTerminalInputParams {
    pub terminal_id: String,
    pub data: String,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpTerminalResizeParams {
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpAttachTerminalParams {
    pub terminal_id: String,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpReadTerminalParams {
    pub terminal_id: String,
    /// 1-500, default 100.
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AcpReadTerminalResponse {
    pub terminal_id: String,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpDelegateTaskParams {
    /// Human-readable name for the Telegram forum topic and metadata.title.
    /// Used by the daemon when auto-creating the topic for this session.
    pub name: String,
    /// Absolute path the spawned ACP session should treat as its working dir.
    pub project_path: String,
    /// Prompt text sent once SessionStarted is observed.
    pub text: String,
    #[serde(default)]
    pub agent_command: Option<String>,
    /// Extra metadata merged into the spawn payload as-is. The Telegram topic
    /// label is set via `bind_telegram_thread { name }` after SessionStarted —
    /// metadata is no longer used for topic naming.
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Option<serde_json::Value>,
    /// Seconds to wait for SessionStarted before giving up. Default 15.
    #[serde(default)]
    pub wait_secs: Option<u64>,
    /// Target ACP instance id. Omit when only one is registered.
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpListDirectoriesParams {
    pub query: String,
    pub session_id: Option<String>,
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpFindFilesParams {
    pub query: String,
    pub session_id: Option<String>,
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpReadFileParams {
    pub path: String,
    pub session_id: Option<String>,
    pub start_line: Option<usize>,
    pub line_count: Option<usize>,
    pub instance: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AcpEventsResponse {
    pub events: Vec<crate::acp_ws::AcpEvent>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AcpSnapshotResponse {
    pub snapshot: Option<crate::acp_ws::AcpEvent>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AcpCommandAck {
    pub ok: bool,
    /// Wire variant the daemon will see (e.g. "spawn_session").
    pub sent: String,
    /// Optional context (e.g. echoed `request_id` for permission_response).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "metadata_schema", default)]
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AcpDelegateTaskResponse {
    pub ok: bool,
    pub session_id: Option<String>,
    /// Topic name passed in. Echoed for caller convenience.
    pub name: String,
    /// True when a `bind_telegram_thread` command was issued after SessionStarted.
    pub telegram_bound: bool,
    /// Resolved Telegram forum topic id, populated from a
    /// `telegram_thread_bound` ack event (telegram-acp ≥ v1.4). `None` when
    /// the daemon hasn't shipped the ack event yet — caller can poll
    /// `acp_recent_events { kinds: ["telegram_thread_bound"] }` instead.
    pub thread_id: Option<i64>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpRenameSessionParams {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AcpRemoveTopicParams {
    pub thread_id: i64,
    #[serde(default)]
    pub instance: Option<String>,
}


#[tool_router(router = tool_router)]
impl AcpMcpServer {

    // --- ACP delegation surface ---

    #[tool(
        description = "List discovered ACP daemon instances (mDNS + HTTP-registered) and the currently selected one. Use the returned `name` with `acp_select_instance` to switch the WS target."
    )]
    async fn acp_list_instances(&self) -> Result<Json<AcpInstancesResponse>, String> {
        let disc = self
            .state
            .acp_discovery
            .as_ref()
            .ok_or_else(|| "acp discovery not enabled".to_string())?;
        let instances = disc.list().await;
        let active = disc.active().await.map(|i| i.name);
        Ok(Json(AcpInstancesResponse { instances, active }))
    }

    #[tool(
        description = "Select an ACP daemon instance by name. The WS client reconnects to the new target. Returns the resolved instance."
    )]
    async fn acp_select_instance(
        &self,
        Parameters(AcpSelectInstanceParams { name }): Parameters<AcpSelectInstanceParams>,
    ) -> Result<Json<crate::acp_discovery::AcpInstance>, String> {
        let disc = self
            .state
            .acp_discovery
            .as_ref()
            .ok_or_else(|| "acp discovery not enabled".to_string())?;
        disc.select(&name)
            .await
            .map(Json)
            .ok_or_else(|| format!("unknown acp instance: {name}"))
    }

    #[tool(
        description = "Ask the target ACP daemon to emit a fresh ListSessions response over WS. Inspect with `acp_recent_events { kinds: [\"ListSessions\"] }`. Pass `instance` to disambiguate when multiple are registered."
    )]
    async fn acp_list_sessions(
        &self,
        Parameters(params): Parameters<AcpInstanceParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command("list_sessions", serde_json::json!({}))
            .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "list_sessions".into(),
            context: None,
        }))
    }

    #[tool(
        description = "Spawn a headless ACP session on the target daemon. Returns immediately; the new session id arrives as a `SessionStarted` event. Use `acp_delegate_task` for one-shot spawn-and-prompt. Pass `instance` when multiple are registered."
    )]
    async fn acp_spawn_session(
        &self,
        Parameters(params): Parameters<AcpSpawnParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let mut payload = serde_json::Map::new();
        payload.insert(
            "project_path".into(),
            serde_json::Value::String(params.project_path),
        );
        if let Some(cmd) = params.agent_command {
            payload.insert("agent_command".into(), serde_json::Value::String(cmd));
        }
        if let Some(meta) = params.metadata {
            payload.insert("metadata".into(), meta);
        }
        h.command("spawn_session", serde_json::Value::Object(payload))
            .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "spawn_session".into(),
            context: None,
        }))
    }

    #[tool(
        description = "Send a prompt to an existing ACP session. Reply text streams back as `AssistantMessage` / `ToolCall` events; poll with `acp_recent_events { session_id }`. Pass `instance` when multiple are registered."
    )]
    async fn acp_send_prompt(
        &self,
        Parameters(params): Parameters<AcpSendPromptParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let mut payload = serde_json::Map::new();
        payload.insert(
            "session_id".into(),
            serde_json::Value::String(params.session_id),
        );
        payload.insert("text".into(), serde_json::Value::String(params.text));
        if let Some(att) = params.attachments {
            payload.insert(
                "attachments".into(),
                serde_json::Value::Array(att.into_iter().map(serde_json::Value::String).collect()),
            );
        }
        h.command("send_prompt", serde_json::Value::Object(payload))
            .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "send_prompt".into(),
            context: None,
        }))
    }

    #[tool(description = "Cancel the currently running prompt on an ACP session.")]
    async fn acp_cancel(
        &self,
        Parameters(params): Parameters<AcpSessionIdParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "cancel",
            serde_json::json!({ "session_id": params.session_id }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "cancel".into(),
            context: None,
        }))
    }

    #[tool(
        description = "Gracefully terminate an ACP session. Provide session_id (preferred) or thread_id fallback."
    )]
    async fn acp_end_session(
        &self,
        Parameters(params): Parameters<AcpEndSessionParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let mut payload = serde_json::Map::new();
        if let Some(s) = params.session_id {
            payload.insert("session_id".into(), serde_json::Value::String(s));
        }
        if let Some(t) = params.thread_id {
            payload.insert("thread_id".into(), serde_json::Value::Number(t.into()));
        }
        if payload.is_empty() {
            return Err("session_id or thread_id required".into());
        }
        h.command("end_session", serde_json::Value::Object(payload))
            .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "end_session".into(),
            context: None,
        }))
    }

    #[tool(description = "Update the session/topic name.")]
    async fn acp_rename_session(
        &self,
        Parameters(params): Parameters<AcpRenameSessionParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "rename_session",
            serde_json::json!({ "session_id": params.session_id, "name": params.name }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "rename_session".into(),
            context: None,
        }))
    }

    #[tool(
        description = "Removes a topic (and its session history) from the daemon's memory and disk."
    )]
    async fn acp_remove_topic(
        &self,
        Parameters(params): Parameters<AcpRemoveTopicParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "remove_topic",
            serde_json::json!({ "thread_id": params.thread_id }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "remove_topic".into(),
            context: None,
        }))
    }

    #[tool(
        description = "Switch a session between auto and manual tool-call approval (`mode`: \"auto\" | \"manual\")."
    )]
    async fn acp_set_permission_mode(
        &self,
        Parameters(params): Parameters<AcpSetPermissionModeParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "set_permission_mode",
            serde_json::json!({ "session_id": params.session_id, "mode": params.mode }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "set_permission_mode".into(),
            context: None,
        }))
    }

    #[tool(
        description = "Reply to an outstanding PermissionRequest. `decision` ∈ allow_once | allow_always | deny | deny_always."
    )]
    async fn acp_permission_respond(
        &self,
        Parameters(params): Parameters<AcpPermissionRespondParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "permission_response",
            serde_json::json!({ "request_id": params.request_id, "decision": params.decision }),
        )
        .map_err(|e| e.to_string())?;
        h.mark_permission_resolved(&params.request_id).await;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "permission_response".into(),
            context: Some(serde_json::json!({ "request_id": params.request_id })),
        }))
    }

    #[tool(
        description = "Read recent ACP WS events from the in-process ring buffer. Filter by session_id, since_local_seq, or kinds. Buffers up to ~200 events per session."
    )]
    async fn acp_recent_events(
        &self,
        Parameters(params): Parameters<AcpRecentEventsParams>,
    ) -> Result<Json<AcpEventsResponse>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let events = h
            .recent_events(
                params.session_id.as_deref(),
                params.terminal_id.as_deref(),
                params.since_local_seq,
                params.kinds.as_deref(),
                params.limit,
            )
            .await;
        Ok(Json(AcpEventsResponse { events }))
    }

    #[tool(description = "List outstanding PermissionRequest events awaiting a decision.")]
    async fn acp_pending_permissions(
        &self,
        Parameters(params): Parameters<AcpInstanceParams>,
    ) -> Result<Json<AcpEventsResponse>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        Ok(Json(AcpEventsResponse {
            events: h.pending_permissions().await,
        }))
    }

    #[tool(
        description = "Return the most recent Snapshot event (full session state) the WS client has seen, or null if none yet."
    )]
    async fn acp_get_snapshot(
        &self,
        Parameters(params): Parameters<AcpInstanceParams>,
    ) -> Result<Json<AcpSnapshotResponse>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        Ok(Json(AcpSnapshotResponse {
            snapshot: h.latest_snapshot().await,
        }))
    }

    #[tool(
        description = "Create a new terminal in an ACP session. Returns the terminal_id on success. \
Wait for the terminal_created event before returning (default 10s timeout)."
    )]
    async fn acp_create_terminal(
        &self,
        Parameters(params): Parameters<AcpCreateTerminalParams>,
    ) -> Result<Json<AcpCreateTerminalResponse>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let mut rx = h.subscribe();

        let mut payload = serde_json::Map::new();
        if let Some(sid) = params.session_id {
            payload.insert("session_id".into(), serde_json::Value::String(sid));
        }
        payload.insert("cwd".into(), serde_json::Value::String(params.cwd));
        if let Some(cols) = params.cols {
            payload.insert("cols".into(), serde_json::Value::Number(cols.into()));
        }
        if let Some(rows) = params.rows {
            payload.insert("rows".into(), serde_json::Value::Number(rows.into()));
        }

        h.command("create_terminal", serde_json::Value::Object(payload))
            .map_err(|e| e.to_string())?;

        let wait = Duration::from_secs(params.wait_secs.unwrap_or(10).clamp(1, 60));
        let deadline = tokio::time::Instant::now() + wait;

        let terminal_id =
            wait_for_event(&mut rx, deadline, |frame| parse_terminal_created(frame)).await;

        let Some(tid) = terminal_id else {
            return Ok(Json(AcpCreateTerminalResponse {
                ok: false,
                terminal_id: None,
                note: Some("terminal_created event not observed within wait window".into()),
            }));
        };

        Ok(Json(AcpCreateTerminalResponse {
            ok: true,
            terminal_id: Some(tid),
            note: None,
        }))
    }

    #[tool(description = "Gracefully close an ACP terminal.")]
    async fn acp_close_terminal(
        &self,
        Parameters(params): Parameters<AcpTerminalIdParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "close_terminal",
            serde_json::json!({ "terminal_id": params.terminal_id }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "close_terminal".into(),
            context: Some(serde_json::json!({ "terminal_id": params.terminal_id })),
        }))
    }

    #[tool(
        description = "Send input to an ACP terminal. `data` is a plain string that will be base64-encoded before sending."
    )]
    async fn acp_terminal_input(
        &self,
        Parameters(params): Parameters<AcpTerminalInputParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let b64_data = BASE64.encode(params.data);
        h.command(
            "terminal_input",
            serde_json::json!({
                "terminal_id": params.terminal_id,
                "data": b64_data,
            }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "terminal_input".into(),
            context: Some(serde_json::json!({ "terminal_id": params.terminal_id })),
        }))
    }

    #[tool(
        description = "Read recent output from an ACP terminal. Aggregates `terminal_output` and `terminal_snapshot` events, decodes base64 data, and returns a plain text block."
    )]
    async fn acp_read_terminal(
        &self,
        Parameters(params): Parameters<AcpReadTerminalParams>,
    ) -> Result<Json<AcpReadTerminalResponse>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let limit = params.limit.unwrap_or(100).clamp(1, 500);
        let events = h
            .recent_events(
                None,
                Some(&params.terminal_id),
                None,
                Some(&["terminal_output".into(), "terminal_snapshot".into()]),
                Some(limit),
            )
            .await;

        let mut combined = String::new();
        for ev in events {
            if let Some(b64) = ev.payload.get("data").and_then(|v| v.as_str()) {
                if let Ok(bytes) = BASE64.decode(b64) {
                    let text = String::from_utf8_lossy(&bytes);
                    if ev.kind == "terminal_snapshot" {
                        combined = text.into_owned();
                    } else {
                        combined.push_str(&text);
                    }
                }
            }
        }

        Ok(Json(AcpReadTerminalResponse {
            terminal_id: params.terminal_id,
            text: combined,
        }))
    }

    #[tool(
        description = "Attach to an existing ACP terminal. The daemon will emit a `terminal_snapshot` and subsequent `terminal_output` events."
    )]
    async fn acp_attach_terminal(
        &self,
        Parameters(params): Parameters<AcpAttachTerminalParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let mut payload = serde_json::Map::new();
        payload.insert(
            "terminal_id".into(),
            serde_json::Value::String(params.terminal_id.clone()),
        );
        if let Some(cols) = params.cols {
            payload.insert("cols".into(), serde_json::Value::Number(cols.into()));
        }
        if let Some(rows) = params.rows {
            payload.insert("rows".into(), serde_json::Value::Number(rows.into()));
        }

        h.command("attach_terminal", serde_json::Value::Object(payload))
            .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "attach_terminal".into(),
            context: Some(serde_json::json!({ "terminal_id": params.terminal_id })),
        }))
    }

    #[tool(description = "Resize an existing ACP terminal.")]
    async fn acp_terminal_resize(
        &self,
        Parameters(params): Parameters<AcpTerminalResizeParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "terminal_resize",
            serde_json::json!({
                "terminal_id": params.terminal_id,
                "cols": params.cols,
                "rows": params.rows,
            }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "terminal_resize".into(),
            context: Some(serde_json::json!({ "terminal_id": params.terminal_id })),
        }))
    }

    #[tool(
        description = "One-shot delegation: spawn a headless ACP session in `project_path`, wait for SessionStarted (default 15s), bind a fresh Telegram forum topic named `name`, then send `text` as a prompt. `name` becomes metadata.title/metadata.name on spawn so the daemon can label the auto-created topic. Returns the new session_id, or `ok=false` with a hint if SessionStarted didn't arrive in time."
    )]
    async fn acp_delegate_task(
        &self,
        Parameters(params): Parameters<AcpDelegateTaskParams>,
    ) -> Result<Json<AcpDelegateTaskResponse>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        let name = params.name.trim().to_string();
        if name.is_empty() {
            return Err("name must not be empty".into());
        }
        if name.chars().count() > 128 {
            return Err("name exceeds Telegram forum-topic cap (128 chars)".into());
        }
        if name.chars().any(|c| c.is_control()) {
            return Err("name must not contain control characters".into());
        }
        let mut rx = h.subscribe();

        let mut spawn_payload = serde_json::Map::new();
        spawn_payload.insert(
            "project_path".into(),
            serde_json::Value::String(params.project_path),
        );
        if let Some(cmd) = params.agent_command {
            spawn_payload.insert("agent_command".into(), serde_json::Value::String(cmd));
        }
        if let Some(meta) = params.metadata {
            spawn_payload.insert("metadata".into(), meta);
        }

        h.command("spawn_session", serde_json::Value::Object(spawn_payload))
            .map_err(|e| e.to_string())?;

        let wait = Duration::from_secs(params.wait_secs.unwrap_or(15).clamp(1, 120));
        let deadline = tokio::time::Instant::now() + wait;

        let session_id =
            wait_for_event(&mut rx, deadline, |frame| parse_session_started(frame)).await;

        let Some(sid) = session_id else {
            return Ok(Json(AcpDelegateTaskResponse {
                ok: false,
                session_id: None,
                name,
                telegram_bound: false,
                thread_id: None,
                note: Some(
                    "SessionStarted not observed within wait window; poll acp_recent_events for SessionStarted, then bind_telegram_thread + send_prompt manually".into(),
                ),
            }));
        };

        // bind_telegram_thread v1.4: { session_id, thread_id: null, name }.
        // Daemon validates `name` (1..=128, no control chars) and emits a
        // `telegram_thread_bound` ack carrying the resolved thread_id.
        let bind_ok = h
            .command(
                "bind_telegram_thread",
                serde_json::json!({
                    "session_id": sid,
                    "thread_id": serde_json::Value::Null,
                    "name": name,
                }),
            )
            .is_ok();

        // Short window to capture the ack. v1.3 daemons won't emit it — fall
        // through to send_prompt with thread_id = None.
        let bind_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let target_sid = sid.clone();
        let thread_id = if bind_ok {
            wait_for_event(&mut rx, bind_deadline, |frame| {
                parse_telegram_thread_bound(frame, &target_sid)
            })
            .await
        } else {
            None
        };

        h.command(
            "send_prompt",
            serde_json::json!({ "session_id": sid, "text": params.text }),
        )
        .map_err(|e| e.to_string())?;

        Ok(Json(AcpDelegateTaskResponse {
            ok: true,
            session_id: Some(sid),
            name,
            telegram_bound: bind_ok,
            thread_id,
            note: None,
        }))
    }

    #[tool(description = "Request directory suggestions for path typeahead.")]
    async fn acp_list_directories(
        &self,
        Parameters(params): Parameters<AcpListDirectoriesParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "list_directories",
            serde_json::json!({ "query": params.query, "session_id": params.session_id }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "list_directories".into(),
            context: None,
        }))
    }

    #[tool(description = "Request fuzzy file search results in the current project directory.")]
    async fn acp_find_files(
        &self,
        Parameters(params): Parameters<AcpFindFilesParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "find_files",
            serde_json::json!({ "query": params.query, "session_id": params.session_id }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "find_files".into(),
            context: None,
        }))
    }

    #[tool(description = "Request file contents from the current project or an absolute path.")]
    async fn acp_read_file(
        &self,
        Parameters(params): Parameters<AcpReadFileParams>,
    ) -> Result<Json<AcpCommandAck>, String> {
        let h = require_acp(&self.state, params.instance.as_deref()).await?;
        h.command(
            "read_file",
            serde_json::json!({
                "path": params.path,
                "session_id": params.session_id,
                "start_line": params.start_line,
                "line_count": params.line_count
            }),
        )
        .map_err(|e| e.to_string())?;
        Ok(Json(AcpCommandAck {
            ok: true,
            sent: "read_file".into(),
            context: None,
        }))
    }

}

async fn require_acp(
    state: &AppState,
    instance: Option<&str>,
) -> Result<std::sync::Arc<crate::acp_ws::AcpWsHandle>, String> {
    let registry = state
        .acp_ws
        .as_ref()
        .ok_or_else(|| "ACP WS registry not initialized".to_string())?;
    if let Some(worker) = registry.resolve(instance).await {
        return Ok(worker);
    }
    let n = registry.len().await;
    if n == 0 {
        Err("no ACP instances registered; start a daemon or POST /api/acp/register".to_string())
    } else if instance.is_some() {
        Err(format!(
            "unknown ACP instance '{}'",
            instance.unwrap_or("?")
        ))
    } else {
        Err(format!(
            "multiple ACP instances registered ({n}); specify `instance` (see /api/acp/instances)"
        ))
    }
}

/// Drain frames from a daemon broadcast subscriber until `extract` returns
/// `Some(_)` or the deadline elapses. Handles `Lagged` by continuing.
async fn wait_for_event<T, F>(
    rx: &mut tokio::sync::broadcast::Receiver<String>,
    deadline: tokio::time::Instant,
    mut extract: F,
) -> Option<T>
where
    F: FnMut(&str) -> Option<T>,
{
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(text)) => {
                if let Some(v) = extract(&text) {
                    return Some(v);
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

/// Pull `(kind, payload_object)` out of a daemon frame. Tolerates both
/// `{ "Variant": {...} }` and `{ "type"|"kind": "variant", ... }` shapes.
fn extract_kind_payload(
    text: &str,
) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let map = value.as_object()?;
    if map.len() == 1 {
        let (k, v) = map.iter().next()?;
        let payload = v.as_object()?.clone();
        return Some((k.clone(), payload));
    }
    let kind = map
        .get("type")
        .or_else(|| map.get("kind"))
        .and_then(|v| v.as_str())?
        .to_owned();
    Some((kind, map.clone()))
}

fn kind_eq(actual: &str, snake: &str, camel: &str) -> bool {
    actual.eq_ignore_ascii_case(camel) || actual == snake
}

/// Best-effort parse of a daemon WS frame to extract `acp_session_id` from a
/// SessionStarted envelope.
fn parse_session_started(text: &str) -> Option<String> {
    let (kind, payload) = extract_kind_payload(text)?;
    if !kind_eq(&kind, "session_started", "SessionStarted") {
        return None;
    }
    payload
        .get("acp_session_id")
        .or_else(|| payload.get("session_id"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

fn parse_terminal_created(text: &str) -> Option<String> {
    let (kind, payload) = extract_kind_payload(text)?;
    if !kind_eq(&kind, "terminal_created", "TerminalCreated") {
        return None;
    }
    payload
        .get("terminal_id")
        .or_else(|| payload.get("terminal").and_then(|t| t.get("terminal_id")))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// Parse the v1.4 `telegram_thread_bound` ack. Returns the resolved thread_id
/// only when the event references the session we just bound.
fn parse_telegram_thread_bound(text: &str, expect_session: &str) -> Option<i64> {
    let (kind, payload) = extract_kind_payload(text)?;
    if !kind_eq(&kind, "telegram_thread_bound", "TelegramThreadBound") {
        return None;
    }
    let sid = payload
        .get("session_id")
        .or_else(|| payload.get("acp_session_id"))
        .and_then(|v| v.as_str())?;
    if sid != expect_session {
        return None;
    }
    payload.get("thread_id").and_then(|v| v.as_i64())
}

/// Build the `StreamableHttpService` tower service that serves ACP MCP traffic
/// at `/mcp/acp`. Same guards as the main endpoint: allowed Host headers and
/// SSE keep-alive.
pub fn streamable_http_service(
    state: AppState,
) -> StreamableHttpService<AcpMcpServer, LocalSessionManager> {
    let allowed_hosts = state.mcp_allowed_hosts();
    let factory_state = state;
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts)
        .with_sse_keep_alive(Some(Duration::from_secs(15)))
        .with_stateful_mode(false)
        .with_json_response(true);
    StreamableHttpService::new(
        move || Ok(AcpMcpServer::new(factory_state.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}

#[cfg(test)]
mod tests {
    use super::AcpCommandAck;
    use schemars::schema_for;

    #[test]
    fn acp_command_ack_context_is_not_required() {
        let schema = serde_json::to_value(schema_for!(AcpCommandAck)).expect("serialize schema");
        let required = schema
            .pointer("/required")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        assert!(
            !required
                .iter()
                .any(|value| value.as_str() == Some("context")),
            "context should stay optional in the generated schema: {schema}"
        );
    }
}

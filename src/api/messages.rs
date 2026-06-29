use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::db::{
    ChannelSummary, MessageQuery, MessageRecord, MessageSenderKind, MessageUpdate, NewMessage,
    SortOrder,
};

use super::auth::SessionSubject;
use super::auth_guard::{AuthKind, RequestAuthContext, is_admin_subject};
use super::error::{ApiError, current_timestamp_millis, metadata_schema};
use super::presence::PresenceEntry;
use super::state::AppState;
use super::store_search::DeleteResponse;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SendMessageRequest {
    /// Channel name (slack-like, e.g. "general", "ops"). Required.
    pub channel: String,
    /// Message body. Optional when `kind != "text"` and `metadata` carries the payload.
    #[serde(default)]
    pub text: String,
    /// Optional sender label. If omitted, the authenticated session subject is used,
    /// falling back to "anonymous".
    #[serde(default)]
    pub sender: Option<String>,
    /// Optional sender kind. Defaults to "human" for sessions, "agent" for MCP/agent tokens.
    #[serde(default)]
    pub sender_kind: Option<MessageSenderKind>,
    /// Message kind. Free-form lowercase string. Common values:
    /// `text` (default), `permission_request`, `permission_response`, `tool_call`,
    /// `agent_chunk`, `agent_root_discovery`. Used by clients to route rendering and by ACP bridges to
    /// distinguish protocol traffic.
    #[serde(default)]
    pub kind: Option<String>,
    /// Free-form JSON metadata. For `permission_request` use:
    /// `{"request_id": "...", "options": [{"option_id","name","kind"}], "tool_call": {...}}`.
    /// For `permission_response`: `{"request_id": "...", "option_id": "..."}`.
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListMessagesQuery {
    pub channel: Option<String>,
    pub sender: Option<String>,
    /// Filter by message kind (e.g. "text", "permission_request").
    pub kind: Option<String>,
    /// Inclusive lower bound on created_at (ms since epoch).
    pub since: Option<i64>,
    /// Inclusive upper bound on created_at (ms since epoch).
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_order: Option<SortOrder>,
    /// Display name of the polling caller. When set together with `channel`,
    /// the caller is registered as active in that channel and the response
    /// includes the current `active_users` list for the channel.
    pub user: Option<String>,
    /// Optional sender_kind for the polling caller. Defaults to "human".
    pub user_kind: Option<MessageSenderKind>,
    /// Long-poll wait in seconds (max 30). When set and the initial query
    /// returns no messages, the request blocks until either a new message
    /// arrives or the timeout elapses, then re-runs the query once.
    pub wait: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct MessagePayload {
    pub id: String,
    pub channel: String,
    pub sender: String,
    pub sender_kind: MessageSenderKind,
    pub text: String,
    pub kind: String,
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UpdateMessageRequest {
    /// Replacement text. When `append` is true, this is appended to the
    /// existing body instead of replacing it.
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    #[schemars(schema_with = "metadata_schema")]
    pub metadata: Option<Value>,
    /// Append `text` to the existing body (default: replace).
    #[serde(default)]
    pub append: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ActiveUserPayload {
    pub user: String,
    pub kind: String,
    pub last_seen: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct MessagesResponse {
    pub messages: Vec<MessagePayload>,
    pub total_count: i64,
    /// Active users in the requested channel (presence updated by polls). Empty
    /// when no `channel` filter is provided.
    #[serde(default)]
    pub active_users: Vec<ActiveUserPayload>,
    /// Ids of messages deleted since the request's `since` cursor (server-side
    /// in-memory tombstones, ~5 min retention). Frontend should drop these
    /// from local state.
    #[serde(default)]
    pub deleted_ids: Vec<String>,
}

impl From<PresenceEntry> for ActiveUserPayload {
    fn from(value: PresenceEntry) -> Self {
        Self {
            user: value.user,
            kind: value.kind,
            last_seen: value.last_seen,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ChannelPayload {
    pub channel: String,
    pub message_count: i64,
    pub last_message_at: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ChannelsResponse {
    pub channels: Vec<ChannelPayload>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ClearChannelResponse {
    pub channel: String,
    pub deleted_count: usize,
}

impl From<MessageRecord> for MessagePayload {
    fn from(value: MessageRecord) -> Self {
        Self {
            id: value.id,
            channel: value.channel,
            sender: value.sender,
            sender_kind: value.sender_kind,
            text: value.text,
            kind: value.kind,
            metadata: value.metadata,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<ChannelSummary> for ChannelPayload {
    fn from(value: ChannelSummary) -> Self {
        Self {
            channel: value.channel,
            message_count: value.message_count,
            last_message_at: value.last_message_at,
        }
    }
}

pub(crate) async fn send_message(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    Extension(auth): Extension<RequestAuthContext>,
    Json(request): Json<SendMessageRequest>,
) -> Result<(StatusCode, Json<MessagePayload>), ApiError> {
    if request.channel.trim().is_empty() {
        return Err(ApiError::BadRequest("channel cannot be empty".to_owned()));
    }

    let kind = request
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("text")
        .to_owned();
    let metadata = if request.metadata.is_null() {
        Value::Object(Default::default())
    } else {
        request.metadata
    };
    if !metadata.is_object() {
        return Err(ApiError::BadRequest(
            "metadata must be a JSON object".to_owned(),
        ));
    }
    let metadata_empty = match &metadata {
        Value::Object(map) => map.is_empty(),
        _ => true,
    };
    if request.text.trim().is_empty() && metadata_empty {
        return Err(ApiError::BadRequest("text or metadata required".to_owned()));
    }

    let sender = resolve_message_sender(&auth, request.sender.as_deref(), session.0.as_deref())?;
    let sender_kind = resolve_message_sender_kind(&auth, request.sender_kind)?;
    let metadata = stamp_message_auth_metadata(metadata, &auth);

    let new_message = NewMessage {
        id: Uuid::now_v7().to_string(),
        channel: request.channel,
        sender,
        sender_kind,
        text: request.text,
        kind,
        metadata,
        created_at: current_timestamp_millis()?,
    };

    let messages = state.messages.clone();
    let record = tokio::task::spawn_blocking(move || messages.send_message(new_message))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    // Auto-purge: when an answer (permission_response) lands, the originating
    // permission_request is no longer actionable. Delete it so the channel
    // doesn't accumulate stale dialog rows. Tombstone fires so polls cull it
    // from local state.
    if record.kind == "permission_response" {
        if let Some(request_id) = record
            .metadata
            .get("request_id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
        {
            let messages_for_purge = state.messages.clone();
            let tombstones = state.tombstones.clone();
            let request_id_clone = request_id.clone();
            let purged: Vec<MessageRecord> = tokio::task::spawn_blocking(move || {
                let found = messages_for_purge.find_permission_request(&request_id_clone)?;
                let mut deleted = Vec::with_capacity(found.len());
                for row in found {
                    if let Some(d) = messages_for_purge.delete_message(&row.id)? {
                        deleted.push(d);
                    }
                }
                anyhow::Ok(deleted)
            })
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
            for row in &purged {
                tombstones.record(&row.channel, &row.id);
            }
        }
    }

    state.publish_message(&record);

    Ok((StatusCode::CREATED, Json(record.into())))
}

pub(crate) async fn delete_message(
    State(state): State<AppState>,
    Extension(auth): Extension<RequestAuthContext>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, ApiError> {
    let messages = state.messages.clone();
    let id_clone = id.clone();
    let existing = tokio::task::spawn_blocking(move || messages.get_message(&id_clone))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    let Some(existing) = existing else {
        return Err(ApiError::NotFound(format!("message {id} not found")));
    };
    authorize_message_mutation(&auth, &existing, "delete")?;

    let messages = state.messages.clone();
    let id_clone = id.clone();
    let deleted = tokio::task::spawn_blocking(move || messages.delete_message(&id_clone))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    let Some(record) = deleted else {
        return Err(ApiError::NotFound(format!("message {id} not found")));
    };
    state.tombstones.record(&record.channel, &record.id);
    state.message_notify.notify_waiters();
    Ok(Json(DeleteResponse {
        id: record.id,
        deleted: true,
    }))
}

pub(crate) async fn list_messages(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    Extension(auth): Extension<RequestAuthContext>,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<MessagesResponse>, ApiError> {
    let messages = state.messages.clone();
    let build_request = || MessageQuery {
        channel: query.channel.clone(),
        sender: query.sender.clone(),
        kind: query.kind.clone(),
        min_created_at: query.since,
        max_created_at: query.until,
        limit: query.limit,
        offset: query.offset,
        sort_order: query.sort_order.unwrap_or(SortOrder::Desc),
    };

    let run_query = |req: MessageQuery| {
        let messages = messages.clone();
        async move {
            tokio::task::spawn_blocking(move || messages.list_messages(req))
                .await
                .map_err(ApiError::TaskJoin)?
                .map_err(ApiError::Internal)
        }
    };

    let (mut records, mut total_count) = run_query(build_request()).await?;

    let wait_secs = query.wait.unwrap_or(0).min(30);
    if wait_secs > 0 && records.is_empty() {
        let notified = state.message_notify.notified();
        tokio::pin!(notified);
        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(wait_secs), &mut notified).await;
        let (r, t) = run_query(build_request()).await?;
        records = r;
        total_count = t;
    }

    let active_users = if let Some(channel) = query.channel.as_deref() {
        let presence = resolve_presence_identity(&auth, &query, session.0.as_deref())?;
        if let Some((user, kind)) = presence {
            state.presence.touch(channel, &user, kind);
        }
        state
            .presence
            .list(channel)
            .into_iter()
            .map(Into::into)
            .collect()
    } else {
        Vec::new()
    };

    let deleted_ids = if let (Some(channel), Some(since)) = (query.channel.as_deref(), query.since)
    {
        state
            .tombstones
            .since(channel, since)
            .into_iter()
            .map(|t| t.id)
            .collect()
    } else {
        Vec::new()
    };

    Ok(Json(MessagesResponse {
        messages: records.into_iter().map(Into::into).collect(),
        total_count,
        active_users,
        deleted_ids,
    }))
}

pub(crate) async fn update_message(
    State(state): State<AppState>,
    Extension(auth): Extension<RequestAuthContext>,
    Path(id): Path<String>,
    Json(request): Json<UpdateMessageRequest>,
) -> Result<Json<MessagePayload>, ApiError> {
    if request.text.is_none() && request.metadata.is_none() {
        return Err(ApiError::BadRequest(
            "must supply text or metadata".to_owned(),
        ));
    }
    if let Some(metadata) = &request.metadata {
        if !metadata.is_object() {
            return Err(ApiError::BadRequest(
                "metadata must be a JSON object".to_owned(),
            ));
        }
    }

    let messages = state.messages.clone();
    let id_lookup = id.clone();
    let existing = tokio::task::spawn_blocking(move || messages.get_message(&id_lookup))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    let Some(existing) = existing else {
        return Err(ApiError::NotFound(format!("message {id} not found")));
    };
    authorize_message_mutation(&auth, &existing, "update")?;

    let messages = state.messages.clone();
    let now = current_timestamp_millis()?;
    let update = MessageUpdate {
        text: request.text,
        metadata: request.metadata,
        append_text: request.append,
    };

    let record = tokio::task::spawn_blocking({
        let id = id.clone();
        move || messages.update_message(&id, update, now)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?
    .ok_or_else(|| ApiError::NotFound(format!("message {id} not found")))?;

    state.message_notify.notify_waiters();
    Ok(Json(record.into()))
}

pub(crate) async fn list_message_channels(
    State(state): State<AppState>,
) -> Result<Json<ChannelsResponse>, ApiError> {
    let messages = state.messages.clone();
    let channels = tokio::task::spawn_blocking(move || messages.list_channels())
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(ChannelsResponse {
        channels: channels.into_iter().map(Into::into).collect(),
    }))
}

pub(crate) async fn clear_message_channel(
    State(state): State<AppState>,
    Extension(auth): Extension<RequestAuthContext>,
    Path(channel): Path<String>,
) -> Result<Json<ClearChannelResponse>, ApiError> {
    let trimmed = channel.trim().to_owned();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("channel must not be empty".to_owned()));
    }
    let messages = state.messages.clone();
    let preview_target = trimmed.clone();
    let existing =
        tokio::task::spawn_blocking(move || messages.list_channel_messages(&preview_target))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
    authorize_channel_clear(&auth, &trimmed, &existing)?;

    let messages = state.messages.clone();
    let target = trimmed.clone();
    let wiped = tokio::task::spawn_blocking(move || messages.clear_channel(&target))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    for row in &wiped {
        state.tombstones.record(&row.channel, &row.id);
    }
    if !wiped.is_empty() {
        state.message_notify.notify_waiters();
    }

    Ok(Json(ClearChannelResponse {
        channel: trimmed,
        deleted_count: wiped.len(),
    }))
}

const MESSAGE_AUTH_METADATA_KEY: &str = "__auth";

pub(crate) fn resolve_message_sender(
    auth: &RequestAuthContext,
    requested_sender: Option<&str>,
    session_subject: Option<&str>,
) -> Result<String, ApiError> {
    let requested = requested_sender
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match auth.kind {
        AuthKind::SessionCookie => {
            let subject = auth
                .subject
                .as_deref()
                .or(session_subject)
                .ok_or_else(|| ApiError::Unauthorized("session subject missing".to_owned()))?;
            if let Some(sender) = requested {
                if sender != subject {
                    return Err(ApiError::BadRequest(
                        "session-authenticated requests cannot override sender".to_owned(),
                    ));
                }
            }
            Ok(subject.to_owned())
        }
        AuthKind::McpToken => Ok(requested
            .map(ToOwned::to_owned)
            .or_else(|| auth.subject.clone())
            .unwrap_or_else(|| "agent".to_owned())),
        AuthKind::ApiKey | AuthKind::Disabled => Ok(requested
            .map(ToOwned::to_owned)
            .or_else(|| auth.subject.clone())
            .unwrap_or_else(|| "anonymous".to_owned())),
    }
}

pub(crate) fn resolve_message_sender_kind(
    auth: &RequestAuthContext,
    requested_kind: Option<MessageSenderKind>,
) -> Result<MessageSenderKind, ApiError> {
    match auth.kind {
        AuthKind::SessionCookie => match requested_kind {
            None | Some(MessageSenderKind::Human) => Ok(MessageSenderKind::Human),
            Some(_) => Err(ApiError::BadRequest(
                "session-authenticated requests must use sender_kind=human".to_owned(),
            )),
        },
        AuthKind::McpToken => Ok(requested_kind.unwrap_or(MessageSenderKind::Agent)),
        AuthKind::ApiKey | AuthKind::Disabled => {
            Ok(requested_kind.unwrap_or(MessageSenderKind::Human))
        }
    }
}

pub(crate) fn stamp_message_auth_metadata(mut metadata: Value, auth: &RequestAuthContext) -> Value {
    if let Some(map) = metadata.as_object_mut() {
        map.insert(
            MESSAGE_AUTH_METADATA_KEY.to_owned(),
            serde_json::json!({
                "subject": auth.subject,
                "kind": auth.kind.as_str(),
            }),
        );
    }
    metadata
}

pub(crate) fn message_owner_subject(record: &MessageRecord) -> Option<&str> {
    record
        .metadata
        .get(MESSAGE_AUTH_METADATA_KEY)
        .and_then(|meta| meta.get("subject"))
        .and_then(Value::as_str)
}

pub(crate) fn authorize_message_mutation(
    auth: &RequestAuthContext,
    record: &MessageRecord,
    action: &str,
) -> Result<(), ApiError> {
    match auth.kind {
        AuthKind::Disabled | AuthKind::ApiKey => Ok(()),
        AuthKind::SessionCookie => {
            let Some(subject) = auth.subject.as_deref() else {
                return Err(ApiError::Unauthorized("session subject missing".to_owned()));
            };
            if is_admin_subject(Some(subject)) {
                return Ok(());
            }
            let owned = message_owner_subject(record) == Some(subject)
                || (record.sender_kind == MessageSenderKind::Human && record.sender == subject);
            if owned {
                Ok(())
            } else {
                Err(ApiError::Unauthorized(format!(
                    "not allowed to {action} this message"
                )))
            }
        }
        AuthKind::McpToken => {
            let Some(subject) = auth.subject.as_deref() else {
                return Err(ApiError::Unauthorized(
                    "token subject required for message mutation".to_owned(),
                ));
            };
            if is_admin_subject(Some(subject)) {
                return Ok(());
            }
            if message_owner_subject(record) == Some(subject) {
                Ok(())
            } else {
                Err(ApiError::Unauthorized(format!(
                    "not allowed to {action} this message"
                )))
            }
        }
    }
}

pub(crate) fn authorize_channel_clear(
    auth: &RequestAuthContext,
    channel: &str,
    records: &[MessageRecord],
) -> Result<(), ApiError> {
    match auth.kind {
        AuthKind::Disabled | AuthKind::ApiKey => Ok(()),
        AuthKind::McpToken | AuthKind::SessionCookie => {
            // Admin allowlist bypasses per-message ownership: a Zitadel
            // user listed in `RAG_ADMIN_SUBJECTS` can wipe channels they
            // don't own messages in (cleanup of test/legacy channels).
            if is_admin_subject(auth.subject.as_deref()) {
                return Ok(());
            }
            if records
                .iter()
                .all(|record| authorize_message_mutation(auth, record, "clear").is_ok())
            {
                Ok(())
            } else {
                Err(ApiError::Unauthorized(format!(
                    "not allowed to clear channel {channel}"
                )))
            }
        }
    }
}

pub(crate) fn resolve_presence_identity(
    auth: &RequestAuthContext,
    query: &ListMessagesQuery,
    session_subject: Option<&str>,
) -> Result<Option<(String, &'static str)>, ApiError> {
    let requested_user = query
        .user
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    match auth.kind {
        AuthKind::SessionCookie => {
            let subject = auth
                .subject
                .as_deref()
                .or(session_subject)
                .ok_or_else(|| ApiError::Unauthorized("session subject missing".to_owned()))?;
            Ok(Some((
                subject.to_owned(),
                MessageSenderKind::Human.as_serialized(),
            )))
        }
        AuthKind::McpToken => Ok(requested_user.or_else(|| auth.subject.clone()).map(|user| {
            (
                user,
                query
                    .user_kind
                    .unwrap_or(MessageSenderKind::Agent)
                    .as_serialized(),
            )
        })),
        AuthKind::ApiKey | AuthKind::Disabled => {
            Ok(requested_user.or_else(|| auth.subject.clone()).map(|user| {
                (
                    user,
                    query
                        .user_kind
                        .unwrap_or(MessageSenderKind::Human)
                        .as_serialized(),
                )
            }))
        }
    }
}

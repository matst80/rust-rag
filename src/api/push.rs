//! Web Push HTTP surface.
//!
//! Endpoints (all session/api-key gated by the parent `require_api_key`):
//!   GET    /api/push/vapid-public-key    — public key for `pushManager.subscribe()`
//!   POST   /api/push/subscribe           — store browser's pushSubscription
//!   GET    /api/push/subscriptions       — list caller's subscriptions
//!   DELETE /api/push/subscriptions/{id}  — remove one of the caller's subs
//!   POST   /api/notify                   — send a notification to a subject
//!
//! The actual encryption + delivery lives in `crate::notify`. This module
//! is just the HTTP shape on top.

use super::{ApiError, AppState, SessionSubject, current_timestamp_millis};
use crate::db::UpsertPushSubscription;
use crate::notify::{NotificationPayload, send};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::HeaderMap,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, JsonSchema)]
pub struct VapidPublicKeyResponse {
    pub public_key: String,
}

pub async fn vapid_public_key(
    State(state): State<AppState>,
) -> Result<Json<VapidPublicKeyResponse>, ApiError> {
    let key = state
        .web_push
        .public_key
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("VAPID_PUBLIC_KEY not configured".into()))?;
    Ok(Json(VapidPublicKeyResponse { public_key: key }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubscribeRequest {
    /// Endpoint URL from `pushSubscription.endpoint`.
    pub endpoint: String,
    pub keys: SubscribeKeys,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubscribeKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SubscriptionView {
    pub id: String,
    pub endpoint: String,
    pub user_agent: Option<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

pub async fn subscribe(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    headers: HeaderMap,
    Json(req): Json<SubscribeRequest>,
) -> Result<Json<SubscriptionView>, ApiError> {
    let subject = require_subject(&subject)?;
    let store = state
        .push
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("push store not wired".into()))?;
    let now = current_timestamp_millis()?;
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let upsert = UpsertPushSubscription {
        subject,
        endpoint: req.endpoint,
        p256dh: req.keys.p256dh,
        auth: req.keys.auth,
        user_agent,
        now,
    };
    let record = tokio::task::spawn_blocking(move || store.upsert_push_subscription(upsert))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(SubscriptionView {
        id: record.id,
        endpoint: record.endpoint,
        user_agent: record.user_agent,
        created_at: record.created_at,
        last_used_at: record.last_used_at,
    }))
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListSubscriptionsResponse {
    pub subscriptions: Vec<SubscriptionView>,
}

pub async fn list_subscriptions(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
) -> Result<Json<ListSubscriptionsResponse>, ApiError> {
    let subject = require_subject(&subject)?;
    let store = state
        .push
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("push store not wired".into()))?;
    let subs = tokio::task::spawn_blocking(move || store.list_push_subscriptions(&subject))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    Ok(Json(ListSubscriptionsResponse {
        subscriptions: subs
            .into_iter()
            .map(|r| SubscriptionView {
                id: r.id,
                endpoint: r.endpoint,
                user_agent: r.user_agent,
                created_at: r.created_at,
                last_used_at: r.last_used_at,
            })
            .collect(),
    }))
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteResponse {
    pub deleted: bool,
}

pub async fn delete_subscription(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, ApiError> {
    let subject = require_subject(&subject)?;
    let store = state
        .push
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("push store not wired".into()))?;
    let deleted =
        tokio::task::spawn_blocking(move || store.delete_push_subscription(&id, &subject))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
    Ok(Json(DeleteResponse { deleted }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NotifyRequest {
    /// Target subject. Omit to send to the calling subject.
    #[serde(default)]
    pub subject: Option<String>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub ttl_secs: Option<u32>,
    #[serde(default)]
    pub urgency: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

pub async fn notify(
    State(state): State<AppState>,
    Extension(caller): Extension<SessionSubject>,
    Json(req): Json<NotifyRequest>,
) -> Result<Json<crate::notify::SendResult>, ApiError> {
    let target = req
        .subject
        .clone()
        .or_else(|| caller.0.clone())
        .ok_or_else(|| {
            ApiError::BadRequest(
                "no target subject (omit `subject` only when authenticated)".into(),
            )
        })?;

    let payload = NotificationPayload {
        title: req.title,
        body: req.body,
        url: req.url,
        tag: req.tag,
        ttl_secs: req.ttl_secs,
        urgency: req.urgency,
        data: req.data,
    };
    let result = send(&state, &target, &payload)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(result))
}

fn require_subject(s: &SessionSubject) -> Result<String, ApiError> {
    s.0.clone()
        .ok_or_else(|| ApiError::Unauthorized("authenticated session required".into()))
}

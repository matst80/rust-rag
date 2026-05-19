//! Web Push (RFC 8030) sender used by every notification surface in the
//! server (HTTP `/api/notify`, MCP `notify_user`, internal callers).
//!
//! Lifecycle of one `send()`:
//!   1. Load all subscriptions for `subject`.
//!   2. For each one, build an ECE-encrypted `WebPushMessage` with a VAPID
//!      signature using the `web-push` crate.
//!   3. POST the encrypted body to `subscription.endpoint` via the shared
//!      `reqwest::Client` (the same one used for every other outbound call).
//!   4. On 410/404 → delete the dead row.
//!   5. On 2xx → bump `last_used_at`.
//!
//! No client features of `web-push` are pulled in — we use it purely for
//! encryption + VAPID and hand the body to reqwest.

use anyhow::{Context, Result, anyhow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;
use web_push::{
    ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessageBuilder,
};

use crate::api::AppState;
use crate::config::WebPushConfig;
use crate::db::PushStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
    /// Optional click-through URL — the service worker opens this when the
    /// notification is clicked.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub url: Option<String>,
    /// Notification tag — re-using a tag replaces an earlier notification
    /// with the same tag instead of stacking. Good for status updates.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tag: Option<String>,
    /// TTL in seconds the push service holds the message if the device is
    /// offline. Default 3600.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ttl_secs: Option<u32>,
    /// "very-low" | "low" | "normal" (default) | "high".
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub urgency: Option<String>,
    /// Optional structured data forwarded to the SW. Use for action ids,
    /// deep links, anything app-specific.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SendResult {
    pub subject: String,
    pub attempted: usize,
    pub delivered: usize,
    pub removed_dead: usize,
    pub failures: Vec<SendFailure>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SendFailure {
    pub subscription_id: String,
    pub error: String,
}

/// Dispatch `payload` to every push subscription registered for `subject`.
/// Best-effort: continues past per-subscription failures, removes
/// permanently-gone endpoints (410/404), and returns a structured summary.
pub async fn send(
    state: &AppState,
    subject: &str,
    payload: &NotificationPayload,
) -> Result<SendResult> {
    let cfg = state.web_push.clone();
    if !cfg.is_configured() {
        return Err(anyhow!(
            "web push not configured (set VAPID_PUBLIC_KEY, VAPID_PRIVATE_KEY, VAPID_SUBJECT)"
        ));
    }
    let store = state
        .push
        .clone()
        .ok_or_else(|| anyhow!("push store not wired"))?;

    let subject_owned = subject.to_owned();
    let store_clone = store.clone();
    let subs = tokio::task::spawn_blocking(move || {
        store_clone.list_push_subscriptions(&subject_owned)
    })
    .await
    .context("join")??;

    let mut result = SendResult {
        subject: subject.to_owned(),
        attempted: subs.len(),
        delivered: 0,
        removed_dead: 0,
        failures: Vec::new(),
    };

    let body_json = serde_json::to_vec(payload).context("serialize payload")?;
    let private_key = cfg.private_key.as_deref().expect("checked by is_configured");
    let vapid_subject = cfg.subject.as_deref().expect("checked by is_configured");

    for sub in subs {
        match deliver_one(
            state,
            &cfg,
            &store,
            &sub.id,
            &sub.endpoint,
            &sub.p256dh,
            &sub.auth,
            &body_json,
            payload,
            private_key,
            vapid_subject,
        )
        .await
        {
            Ok(DeliveryOutcome::Delivered) => {
                result.delivered += 1;
            }
            Ok(DeliveryOutcome::Gone) => {
                result.removed_dead += 1;
            }
            Err(e) => {
                result.failures.push(SendFailure {
                    subscription_id: sub.id.clone(),
                    error: e.to_string(),
                });
                warn!(
                    subscription_id = %sub.id,
                    error = %e,
                    "web push delivery failed"
                );
            }
        }
    }
    Ok(result)
}

enum DeliveryOutcome {
    Delivered,
    Gone,
}

#[allow(clippy::too_many_arguments)]
async fn deliver_one(
    state: &AppState,
    _cfg: &WebPushConfig,
    store: &Arc<dyn PushStore>,
    sub_id: &str,
    endpoint: &str,
    p256dh: &str,
    auth: &str,
    body: &[u8],
    payload: &NotificationPayload,
    vapid_private_key: &str,
    vapid_subject: &str,
) -> Result<DeliveryOutcome> {
    let info = SubscriptionInfo::new(endpoint, p256dh, auth);

    let mut sig_builder = VapidSignatureBuilder::from_base64(vapid_private_key, &info)
        .context("build vapid signature")?;
    sig_builder.add_claim("sub", vapid_subject);
    let signature = sig_builder.build().context("sign vapid")?;

    let mut builder = WebPushMessageBuilder::new(&info);
    builder.set_payload(ContentEncoding::Aes128Gcm, body);
    builder.set_vapid_signature(signature);
    let ttl = payload.ttl_secs.unwrap_or(3600);
    builder.set_ttl(ttl);
    if let Some(u) = payload.urgency.as_deref()
        && let Some(parsed) = parse_urgency(u)
    {
        builder.set_urgency(parsed);
    }
    if let Some(tag) = payload.tag.as_deref() {
        // Topic and tag aren't quite the same but topic is the closest
        // RFC 8030 concept — replaces an earlier message with the same
        // topic while it's queued at the push service.
        builder.set_topic(tag.to_owned());
    }
    let message = builder.build().context("build web push message")?;

    // POST via our shared reqwest client. The encrypted body is in
    // `message.payload.content`; the crypto headers and Content-Encoding
    // come from `payload.crypto_headers` and `payload.content_encoding`.
    let mut req = state
        .http_client
        .post(message.endpoint.to_string())
        .header("TTL", message.ttl.to_string());
    if let Some(u) = message.urgency {
        req = req.header("Urgency", u.to_string());
    }
    if let Some(t) = &message.topic {
        req = req.header("Topic", t);
    }

    if let Some(p) = message.payload {
        req = req.header("Content-Encoding", encoding_header(&p.content_encoding));
        for (name, value) in &p.crypto_headers {
            req = req.header(*name, value);
        }
        req = req.body(p.content);
    } else {
        // Payload-less push — still need a 0-length body.
        req = req.body(Vec::<u8>::new());
    }

    let response = req.send().await.context("send push")?;
    let status = response.status();

    if status.is_success() {
        let store = store.clone();
        let id = sub_id.to_owned();
        let now = crate::api::current_timestamp_millis()?;
        let _ = tokio::task::spawn_blocking(move || store.touch_push_subscription(&id, now)).await;
        Ok(DeliveryOutcome::Delivered)
    } else if status == reqwest::StatusCode::GONE || status == reqwest::StatusCode::NOT_FOUND {
        let store = store.clone();
        let endpoint = endpoint.to_owned();
        let _ = tokio::task::spawn_blocking(move || {
            store.delete_push_subscription_by_endpoint(&endpoint)
        })
        .await;
        Ok(DeliveryOutcome::Gone)
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(anyhow!("push service returned {status}: {body}"))
    }
}

fn parse_urgency(s: &str) -> Option<web_push::Urgency> {
    match s {
        "very-low" => Some(web_push::Urgency::VeryLow),
        "low" => Some(web_push::Urgency::Low),
        "normal" => Some(web_push::Urgency::Normal),
        "high" => Some(web_push::Urgency::High),
        _ => None,
    }
}

fn encoding_header(encoding: &ContentEncoding) -> &'static str {
    match encoding {
        ContentEncoding::Aes128Gcm => "aes128gcm",
        ContentEncoding::AesGcm => "aesgcm",
    }
}

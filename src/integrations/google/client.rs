//! Per-subject Google API client.
//!
//! Loads encrypted OAuth credentials from the vault, decrypts them, and
//! handles transparent access-token refresh + re-encryption on rotation.
//! Build a fresh one per request — it captures a snapshot of credentials at
//! load time and writes any rotation back through the store.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::AppState;
use crate::crypto::EncryptionKey;
use crate::db::{
    OAuthCredentialsRecord, OAuthCredsStore, UpsertOAuthCredentials,
};

const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
/// Refresh access tokens this many seconds before their stated expiry so a
/// burst of calls in flight doesn't race the 401.
const REFRESH_LEEWAY_SECS: i64 = 60;

pub const PROVIDER: &str = "google";

#[derive(Debug, thiserror::Error)]
pub enum GoogleClientError {
    #[error("google is not connected for this subject")]
    NotConnected,
    #[error("server is not configured for the google integration ({0})")]
    NotConfigured(&'static str),
    #[error("token refresh failed: {0}")]
    Refresh(String),
    #[error("upstream google api error ({status}): {body}")]
    Upstream {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub struct GoogleClient {
    state: AppState,
    subject: String,
    access_token: String,
}

impl GoogleClient {
    /// Build a client for `subject`. Loads the row, decrypts both tokens,
    /// refreshes the access token if it's near expiry, and persists the
    /// rotated value back to the vault before returning.
    pub async fn for_subject(state: &AppState, subject: &str) -> Result<Self, GoogleClientError> {
        let store = state
            .oauth_creds
            .clone()
            .ok_or(GoogleClientError::NotConfigured("oauth_creds store not wired"))?;
        let enc_key = state
            .oauth_token_key
            .clone()
            .ok_or(GoogleClientError::NotConfigured("OAUTH_TOKEN_ENC_KEY"))?;

        let subject_owned = subject.to_owned();
        let store_clone = store.clone();
        let record = tokio::task::spawn_blocking(move || {
            store_clone.find_oauth_credentials(&subject_owned, PROVIDER)
        })
        .await
        .map_err(|e| GoogleClientError::Other(anyhow!("join: {e}")))?
        .map_err(GoogleClientError::Other)?
        .ok_or(GoogleClientError::NotConnected)?;

        let access_token =
            decrypt_token(&enc_key, record.access_token_enc.as_deref(), "access_token")?;
        let refresh_token = record
            .refresh_token_enc
            .as_deref()
            .map(|enc| decrypt_token(&enc_key, Some(enc), "refresh_token"))
            .transpose()?;

        let now_ms = current_ms()?;
        let needs_refresh = match record.expires_at {
            Some(expires_at) => expires_at - (REFRESH_LEEWAY_SECS * 1000) <= now_ms,
            // No stored expiry → assume valid; let the first 401 trigger a follow-up.
            None => false,
        };

        let access_token = if needs_refresh {
            let refresh_token = refresh_token.ok_or_else(|| {
                GoogleClientError::Refresh(
                    "no refresh_token stored — user must re-consent".into(),
                )
            })?;
            refresh_and_persist(state, &enc_key, store.clone(), &record, &refresh_token).await?
        } else {
            access_token
        };

        Ok(Self {
            state: state.clone(),
            subject: subject.to_owned(),
            access_token,
        })
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Build an authenticated GET request. The caller adds query parameters
    /// via the returned `reqwest::RequestBuilder`.
    pub fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.state
            .http_client
            .get(url)
            .bearer_auth(&self.access_token)
    }

    /// Send a GET and parse the body as JSON, mapping non-2xx into a typed
    /// `Upstream` error so callers can distinguish quota/auth/other.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<T, GoogleClientError> {
        let response = req
            .send()
            .await
            .map_err(|e| GoogleClientError::Other(anyhow!("send: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GoogleClientError::Upstream { status, body });
        }
        response
            .json::<T>()
            .await
            .map_err(|e| GoogleClientError::Other(anyhow!("parse json: {e}")))
    }

    /// Send a GET and return the raw response bytes. For non-JSON endpoints
    /// like Drive's `files.export` and `files.get?alt=media`.
    pub async fn get_bytes(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), GoogleClientError> {
        let response = req
            .send()
            .await
            .map_err(|e| GoogleClientError::Other(anyhow!("send: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GoogleClientError::Upstream { status, body });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| GoogleClientError::Other(anyhow!("read body: {e}")))?;
        Ok((status, bytes.to_vec()))
    }
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: Option<u64>,
    /// Google may rotate the refresh token; if present, persist it.
    refresh_token: Option<String>,
    scope: Option<String>,
    #[allow(dead_code)]
    token_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct RefreshRequest<'a> {
    grant_type: &'a str,
    client_id: &'a str,
    client_secret: &'a str,
    refresh_token: &'a str,
}

async fn refresh_and_persist(
    state: &AppState,
    enc_key: &EncryptionKey,
    store: Arc<dyn OAuthCredsStore>,
    record: &OAuthCredentialsRecord,
    refresh_token: &str,
) -> Result<String, GoogleClientError> {
    let cfg = state.google_oauth.clone();
    let client_id = cfg
        .client_id
        .as_deref()
        .ok_or(GoogleClientError::NotConfigured("GOOGLE_OAUTH_CLIENT_ID"))?;
    let client_secret = cfg
        .client_secret
        .as_deref()
        .ok_or(GoogleClientError::NotConfigured("GOOGLE_OAUTH_CLIENT_SECRET"))?;

    let body = RefreshRequest {
        grant_type: "refresh_token",
        client_id,
        client_secret,
        refresh_token,
    };
    let response = state
        .http_client
        .post(TOKEN_ENDPOINT)
        .form(&body)
        .send()
        .await
        .map_err(|e| GoogleClientError::Refresh(format!("send: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| GoogleClientError::Refresh(format!("read body: {e}")))?;
    if !status.is_success() {
        return Err(GoogleClientError::Refresh(format!(
            "google returned {status}: {text}"
        )));
    }
    let parsed: RefreshResponse = serde_json::from_str(&text)
        .map_err(|e| GoogleClientError::Refresh(format!("parse: {e}")))?;

    let new_access_enc = enc_key
        .encrypt(parsed.access_token.as_bytes())
        .map_err(GoogleClientError::Other)?;
    let rotated_refresh_enc = parsed
        .refresh_token
        .as_deref()
        .map(|t| enc_key.encrypt(t.as_bytes()))
        .transpose()
        .map_err(GoogleClientError::Other)?;
    let now_ms = current_ms()?;
    let expires_at = parsed.expires_in.map(|secs| now_ms + (secs * 1000) as i64);
    let scopes = parsed.scope.unwrap_or_else(|| record.scopes.clone());

    let upsert = UpsertOAuthCredentials {
        subject: record.subject.clone(),
        provider: record.provider.clone(),
        access_token_enc: Some(new_access_enc),
        // None falls back to existing in the store; only overwrite when
        // google rotated the refresh_token.
        refresh_token_enc: rotated_refresh_enc,
        scopes,
        expires_at,
        account_email: record.account_email.clone(),
        now: now_ms,
    };
    tokio::task::spawn_blocking(move || store.upsert_oauth_credentials(upsert))
        .await
        .map_err(|e| GoogleClientError::Other(anyhow!("join: {e}")))?
        .map_err(GoogleClientError::Other)?;

    Ok(parsed.access_token)
}

fn decrypt_token(
    key: &EncryptionKey,
    enc: Option<&str>,
    kind: &'static str,
) -> Result<String, GoogleClientError> {
    let enc = enc.ok_or_else(|| {
        GoogleClientError::Other(anyhow!("stored {kind} is NULL — re-consent required"))
    })?;
    let raw = key
        .decrypt(enc)
        .with_context(|| format!("decrypt {kind}"))
        .map_err(GoogleClientError::Other)?;
    String::from_utf8(raw)
        .with_context(|| format!("{kind} not valid utf-8"))
        .map_err(GoogleClientError::Other)
}

fn current_ms() -> Result<i64, GoogleClientError> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| GoogleClientError::Other(anyhow!("system clock: {e}")))?
        .as_millis();
    Ok(ms as i64)
}

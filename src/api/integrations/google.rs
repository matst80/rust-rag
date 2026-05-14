//! Google OAuth 2.0 flow handlers (Phase 1).
//!
//! Endpoints:
//!   GET  /api/integrations/google/start      → redirect to Google consent
//!   GET  /api/integrations/google/callback   → exchange code, store encrypted tokens
//!   GET  /api/integrations/google/status     → connection status for the subject
//!   POST /api/integrations/google/disconnect → delete stored credentials
//!
//! State + PKCE verifier are carried across the redirect in a short-lived,
//! HMAC-signed cookie (`rag_google_oauth`) so we don't need a server-side
//! state table. The cookie binds the flow to the authenticated subject.

use crate::api::{ApiError, AppState, SessionSubject, current_timestamp_millis};
use crate::db::UpsertOAuthCredentials;
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill as getrandom_fill;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PROVIDER: &str = "google";
const STATE_COOKIE: &str = "rag_google_oauth";
const STATE_TTL_SECS: i64 = 600;
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const USERINFO_ENDPOINT: &str = "https://openidconnect.googleapis.com/v1/userinfo";

#[derive(Debug, Serialize, Deserialize)]
struct StateClaims {
    sub: String,
    state: String,
    verifier: String,
    return_to: Option<String>,
    exp: i64,
}

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    /// Optional path on the frontend to redirect back to after the flow
    /// completes (e.g. `/settings/integrations`). Defaults to `/`.
    pub return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub connected: bool,
    pub provider: &'static str,
    pub account_email: Option<String>,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DriveSearchQuery {
    pub q: String,
    pub page_size: Option<u32>,
    pub mime_type: Option<String>,
}

pub async fn drive_search(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Query(query): Query<DriveSearchQuery>,
) -> Result<Json<crate::integrations::google::drive::SearchResult>, ApiError> {
    let subject = require_subject(&subject)?;
    let client = crate::integrations::google::GoogleClient::for_subject(&state, &subject)
        .await
        .map_err(ApiError::Internal)?;

    crate::integrations::google::drive::search(
        &client,
        &query.q,
        query.page_size.unwrap_or(20),
        query.mime_type.as_deref(),
    )
    .await
    .map(Json)
    .map_err(ApiError::Internal)
}

pub async fn drive_fetch(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Path(file_id): Path<String>,
) -> Result<Json<crate::integrations::google::drive::FetchedDoc>, ApiError> {
    let subject = require_subject(&subject)?;
    let client = crate::integrations::google::GoogleClient::for_subject(&state, &subject)
        .await
        .map_err(ApiError::Internal)?;

    crate::integrations::google::drive::fetch(&client, &file_id)
        .await
        .map(Json)
        .map_err(ApiError::Internal)
}

#[derive(Debug, Serialize)]
pub struct DisconnectResponse {
    pub deleted: bool,
}

pub async fn status(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
) -> Result<Json<StatusResponse>, ApiError> {
    let subject = require_subject(&subject)?;
    let Some(store) = state.oauth_creds.clone() else {
        return Ok(Json(empty_status()));
    };
    let subject_owned = subject.clone();
    let record = tokio::task::spawn_blocking(move || {
        store.find_oauth_credentials(&subject_owned, PROVIDER)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    Ok(Json(match record {
        Some(r) => StatusResponse {
            connected: true,
            provider: PROVIDER,
            account_email: r.account_email,
            scopes: r.scopes.split(' ').filter(|s| !s.is_empty()).map(str::to_owned).collect(),
            expires_at: r.expires_at,
            updated_at: Some(r.updated_at),
        },
        None => empty_status(),
    }))
}

pub async fn start(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Query(query): Query<StartQuery>,
) -> Result<Response, ApiError> {
    let subject = require_subject(&subject)?;
    let cfg = require_configured(&state)?;
    let secret = state
        .auth
        .session_secret
        .as_deref()
        .ok_or_else(|| ApiError::ServiceUnavailable("session secret required".into()))?;

    let state_token = random_url_token(32);
    let verifier = random_url_token(64);
    let challenge = code_challenge_s256(&verifier);

    let now = current_timestamp_millis()? / 1000;
    let claims = StateClaims {
        sub: subject.clone(),
        state: state_token.clone(),
        verifier,
        return_to: query.return_to.clone(),
        exp: now + STATE_TTL_SECS,
    };
    let jwt = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("sign state cookie: {e}")))?;

    let scope = cfg.default_scopes.join(" ");
    let auth_url = format!(
        "{AUTH_ENDPOINT}?response_type=code\
         &client_id={cid}\
         &redirect_uri={ruri}\
         &scope={scope}\
         &state={state}\
         &code_challenge={chal}\
         &code_challenge_method=S256\
         &access_type=offline\
         &prompt=consent\
         &include_granted_scopes=true",
        cid = urlencode(cfg.client_id.as_deref().unwrap_or_default()),
        ruri = urlencode(cfg.redirect_uri.as_deref().unwrap_or_default()),
        scope = urlencode(&scope),
        state = urlencode(&state_token),
        chal = urlencode(&challenge),
    );

    let cookie = format!(
        "{STATE_COOKIE}={jwt}; Path=/api/integrations/google; Max-Age={STATE_TTL_SECS}; \
         HttpOnly; Secure; SameSite=Lax"
    );
    let mut response = Redirect::temporary(&auth_url).into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    Ok(response)
}

pub async fn callback(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let subject = require_subject(&subject)?;
    let cfg = require_configured(&state)?;
    let secret = state
        .auth
        .session_secret
        .as_deref()
        .ok_or_else(|| ApiError::ServiceUnavailable("session secret required".into()))?;
    let store = state
        .oauth_creds
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("oauth creds store not wired".into()))?;
    let enc_key = state
        .oauth_token_key
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("OAUTH_TOKEN_ENC_KEY not configured".into()))?;

    if let Some(err) = query.error {
        return Err(ApiError::BadRequest(format!("google returned error: {err}")));
    }

    let code = query
        .code
        .ok_or_else(|| ApiError::BadRequest("missing ?code=".into()))?;
    let returned_state = query
        .state
        .ok_or_else(|| ApiError::BadRequest("missing ?state=".into()))?;

    let cookie_jwt = read_cookie(&headers, STATE_COOKIE)
        .ok_or_else(|| ApiError::BadRequest("missing flow state cookie".into()))?;
    let mut validation = Validation::default();
    validation.validate_aud = false;
    let claims = decode::<StateClaims>(
        &cookie_jwt,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| ApiError::BadRequest(format!("invalid flow state cookie: {e}")))?
    .claims;
    if claims.sub != subject {
        return Err(ApiError::BadRequest("flow state subject mismatch".into()));
    }
    if !constant_time_eq(claims.state.as_bytes(), returned_state.as_bytes()) {
        return Err(ApiError::BadRequest("state mismatch".into()));
    }

    let token_resp = exchange_code(&state, &cfg, &code, &claims.verifier).await?;
    let account_email = fetch_account_email(&state, &token_resp.access_token)
        .await
        .ok();

    let access_enc = enc_key
        .encrypt(token_resp.access_token.as_bytes())
        .map_err(ApiError::Internal)?;
    let refresh_enc = token_resp
        .refresh_token
        .as_deref()
        .map(|t| enc_key.encrypt(t.as_bytes()))
        .transpose()
        .map_err(ApiError::Internal)?;
    let now_ms = current_timestamp_millis()?;
    let expires_at = token_resp
        .expires_in
        .map(|secs| now_ms + (secs * 1000) as i64);
    let scopes_str = token_resp.scope.unwrap_or_default();

    let upsert = UpsertOAuthCredentials {
        subject: subject.clone(),
        provider: PROVIDER.into(),
        access_token_enc: Some(access_enc),
        refresh_token_enc: refresh_enc,
        scopes: scopes_str,
        expires_at,
        account_email,
        now: now_ms,
    };
    tokio::task::spawn_blocking(move || store.upsert_oauth_credentials(upsert))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    // Clear the flow cookie.
    let clear_cookie = format!(
        "{STATE_COOKIE}=; Path=/api/integrations/google; Max-Age=0; HttpOnly; Secure; SameSite=Lax"
    );

    let return_to = sanitize_return_to(claims.return_to.as_deref());
    let mut response = Redirect::temporary(&return_to).into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, HeaderValue::from_str(&clear_cookie).unwrap());
    Ok(response)
}

pub async fn disconnect(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
) -> Result<Json<DisconnectResponse>, ApiError> {
    let subject = require_subject(&subject)?;
    let store = state
        .oauth_creds
        .clone()
        .ok_or_else(|| ApiError::ServiceUnavailable("oauth creds store not wired".into()))?;
    let deleted = tokio::task::spawn_blocking(move || {
        store.delete_oauth_credentials(&subject, PROVIDER)
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;
    Ok(Json(DisconnectResponse { deleted }))
}

// ---------- helpers ----------

fn empty_status() -> StatusResponse {
    StatusResponse {
        connected: false,
        provider: PROVIDER,
        account_email: None,
        scopes: Vec::new(),
        expires_at: None,
        updated_at: None,
    }
}

fn require_subject(s: &SessionSubject) -> Result<String, ApiError> {
    s.0.clone()
        .ok_or_else(|| ApiError::Unauthorized("authenticated session required".into()))
}

fn require_configured(
    state: &AppState,
) -> Result<std::sync::Arc<crate::config::GoogleOAuthConfig>, ApiError> {
    let cfg = state.google_oauth.clone();
    if !cfg.is_configured() {
        return Err(ApiError::ServiceUnavailable(
            "google oauth not configured (set GOOGLE_OAUTH_* and OAUTH_TOKEN_ENC_KEY)".into(),
        ));
    }
    Ok(cfg)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    token_type: Option<String>,
}

async fn exchange_code(
    state: &AppState,
    cfg: &crate::config::GoogleOAuthConfig,
    code: &str,
    verifier: &str,
) -> Result<TokenResponse, ApiError> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", cfg.client_id.as_deref().unwrap_or_default()),
        ("client_secret", cfg.client_secret.as_deref().unwrap_or_default()),
        ("redirect_uri", cfg.redirect_uri.as_deref().unwrap_or_default()),
        ("code_verifier", verifier),
    ];
    let resp = state
        .http_client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("token endpoint request: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("read token body: {e}")))?;
    if !status.is_success() {
        return Err(ApiError::BadRequest(format!(
            "google token exchange failed ({status}): {body}"
        )));
    }
    serde_json::from_str::<TokenResponse>(&body)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("parse token body: {e}")))
}

#[derive(Debug, Deserialize)]
struct Userinfo {
    email: Option<String>,
}

async fn fetch_account_email(state: &AppState, access_token: &str) -> anyhow::Result<String> {
    let info: Userinfo = state
        .http_client
        .get(USERINFO_ENDPOINT)
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    info.email
        .ok_or_else(|| anyhow::anyhow!("userinfo has no email"))
}

fn random_url_token(byte_len: usize) -> String {
    let mut buf = vec![0u8; byte_len];
    getrandom_fill(&mut buf).expect("system rng");
    URL_SAFE_NO_PAD.encode(buf)
}

fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for entry in raw.split(';') {
        let mut parts = entry.trim().splitn(2, '=');
        if let (Some(k), Some(v)) = (parts.next(), parts.next())
            && k == name
        {
            return Some(v.to_owned());
        }
    }
    None
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn urlencode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn sanitize_return_to(raw: Option<&str>) -> String {
    match raw {
        Some(p) if p.starts_with('/') && !p.starts_with("//") => p.to_owned(),
        _ => "/".to_owned(),
    }
}

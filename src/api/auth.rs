use super::{ApiError, AppState, current_timestamp_millis};
use crate::db::{DeviceAuthStatus, NewDeviceAuth, NewMcpToken};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{DecodingKey, Validation, decode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

pub(super) const MCP_TOKEN_PREFIX: &str = "rag_mcp_";

#[derive(Clone, Debug)]
pub struct SessionSubject(pub Option<String>);

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeviceCodeRequest {
    pub client_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeviceTokenRequest {
    pub device_code: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeviceTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub token_id: String,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApproveDeviceRequest {
    pub user_code: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApproveDeviceResponse {
    pub token_id: String,
    pub user_code: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerifyDeviceResponse {
    pub user_code: String,
    pub status: String,
    pub client_name: Option<String>,
    pub expires_at: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TokenSummary {
    pub id: String,
    pub name: String,
    pub subject: Option<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListTokensResponse {
    pub tokens: Vec<TokenSummary>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateTokenRequest {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateTokenResponse {
    pub token: String,
    pub id: String,
    pub name: String,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RevokeTokenResponse {
    pub id: String,
    pub deleted: bool,
}

pub(super) fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/device/code", post(device_code))
        .route("/auth/device/token", post(device_token))
        // OAuth-MCP discovery + RFC 6749 token endpoint. Lets MCP HTTP
        // clients (Claude Code, Cursor) auto-discover and run the device
        // flow without hand-pasted tokens. See `oauth_*` handlers below.
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_authorization_server_metadata),
        )
        .route("/oauth/register", post(oauth_register))
        .route("/oauth/token", post(oauth_token))
}

pub(super) fn session_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/auth/device", get(device_approval_page))
        .route("/auth/device/verify", get(verify_device))
        .route("/auth/device/approve", post(approve_device))
        .route("/api/auth/tokens", get(list_tokens).post(create_token))
        .route("/api/auth/tokens/{id}", delete(revoke_token))
        .layer(middleware::from_fn_with_state(state, require_session))
}

async fn require_session(
    State(state): State<AppState>,
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    if !state.auth.is_enabled() {
        request.extensions_mut().insert(SessionSubject(None));
        return Ok(next.run(request).await);
    }

    let Some(secret) = state.auth.session_secret.as_deref() else {
        return Err(ApiError::ServiceUnavailable(
            "session secret is not configured".to_owned(),
        ));
    };

    let cookie_header = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let token = cookie_header
        .split(';')
        .filter_map(|entry| {
            let mut parts = entry.trim().splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some("rag_session"), Some(value)) => Some(value.to_owned()),
                _ => None,
            }
        })
        .next();

    let Some(token) = token else {
        tracing::debug!("session cookie 'rag_session' missing in request headers");
        return Err(ApiError::Unauthorized("session cookie required".to_owned()));
    };

    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_aud = false;

    let claims = decode::<SessionClaims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|error| {
        tracing::warn!(error = %error, "invalid session cookie");
        ApiError::Unauthorized("invalid session cookie".to_owned())
    })?;

    tracing::info!(sub = %claims.claims.sub, "session identified via cookie");

    request
        .extensions_mut()
        .insert(SessionSubject(Some(claims.claims.sub)));

    Ok(next.run(request).await)
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionClaims {
    sub: String,
    exp: usize,
}

async fn device_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<DeviceCodeRequest>>,
) -> Result<Json<DeviceCodeResponse>, ApiError> {
    let client_name = body
        .and_then(|Json(req)| req.client_name)
        .and_then(non_empty);
    let now = current_timestamp_millis()?;
    let ttl_secs = state.auth.device_code_ttl_secs.max(30);
    let interval = state.auth.device_code_interval_secs;
    let expires_at = now + (ttl_secs as i64) * 1000;

    let device_code = random_base64url(32)?;
    let user_code = random_user_code()?;

    let auth_store = state.auth_store.clone();
    let record = NewDeviceAuth {
        device_code: device_code.clone(),
        user_code: user_code.clone(),
        client_name: client_name.clone(),
        created_at: now,
        expires_at,
        interval_secs: interval as i64,
    };
    tokio::task::spawn_blocking(move || auth_store.create_device_auth(record))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    let base = verification_base_url(&state, &headers);
    let verification_uri = format!("{base}/auth/device");
    let verification_uri_complete = format!("{verification_uri}?user_code={user_code}");

    Ok(Json(DeviceCodeResponse {
        device_code,
        user_code,
        verification_uri,
        verification_uri_complete,
        expires_in: ttl_secs,
        interval,
    }))
}

async fn device_token(
    State(state): State<AppState>,
    Json(body): Json<DeviceTokenRequest>,
) -> Response {
    match device_token_inner(state, body).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(status) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": status.as_str() })),
        )
            .into_response(),
    }
}

#[derive(Debug, Clone, Copy)]
enum DeviceTokenError {
    AuthorizationPending,
    SlowDown,
    AccessDenied,
    ExpiredToken,
    InvalidGrant,
    ServerError,
}

impl DeviceTokenError {
    fn as_str(self) -> &'static str {
        match self {
            Self::AuthorizationPending => "authorization_pending",
            Self::SlowDown => "slow_down",
            Self::AccessDenied => "access_denied",
            Self::ExpiredToken => "expired_token",
            Self::InvalidGrant => "invalid_grant",
            Self::ServerError => "server_error",
        }
    }
}

async fn device_token_inner(
    state: AppState,
    body: DeviceTokenRequest,
) -> Result<DeviceTokenResponse, DeviceTokenError> {
    let now = current_timestamp_millis().map_err(|_| DeviceTokenError::ServerError)?;
    let auth_store = state.auth_store.clone();
    let device_code = body.device_code.clone();

    let record = {
        let auth_store = auth_store.clone();
        let device_code = device_code.clone();
        tokio::task::spawn_blocking(move || {
            auth_store.find_device_auth_by_device_code(&device_code)
        })
        .await
        .map_err(|_| DeviceTokenError::ServerError)?
        .map_err(|_| DeviceTokenError::ServerError)?
    }
    .ok_or(DeviceTokenError::InvalidGrant)?;

    // Slow-down enforcement (skipped when interval_secs is 0, e.g. in tests).
    if record.interval_secs > 0 {
        if let Some(last) = record.last_polled_at {
            let elapsed_ms = now - last;
            let min_ms = record.interval_secs * 1000;
            if elapsed_ms < min_ms {
                return Err(DeviceTokenError::SlowDown);
            }
        }
    }

    {
        let auth_store = auth_store.clone();
        let device_code = record.device_code.clone();
        let _ =
            tokio::task::spawn_blocking(move || auth_store.touch_device_poll(&device_code, now))
                .await;
    }

    if record.expires_at <= now {
        let auth_store = auth_store.clone();
        let _ = tokio::task::spawn_blocking(move || auth_store.expire_device_auths(now)).await;
        return Err(DeviceTokenError::ExpiredToken);
    }

    match record.status {
        DeviceAuthStatus::Pending => Err(DeviceTokenError::AuthorizationPending),
        DeviceAuthStatus::Denied => Err(DeviceTokenError::AccessDenied),
        DeviceAuthStatus::Expired => Err(DeviceTokenError::ExpiredToken),
        DeviceAuthStatus::Approved => {
            // Reissue the token plaintext: we stored only the hash, so the only
            // legitimate way to "get" the bearer is the in-flight response when
            // the flow was approved. Since approve_device_auth wrote the token
            // row but the plaintext is gone, we cannot recover it. Instead, we
            // encode the token at approval time into the device request row via
            // token_id + a one-shot plaintext cache.
            //
            // Simpler: the approve endpoint stores the plaintext in a cache
            // keyed by device_code for a single read here.
            let token_id = record.token_id.ok_or(DeviceTokenError::InvalidGrant)?;
            let plaintext = state
                .auth_store_cache()
                .take_pending_token(&record.device_code)
                .ok_or(DeviceTokenError::InvalidGrant)?;

            Ok(DeviceTokenResponse {
                access_token: plaintext,
                token_type: "Bearer".to_owned(),
                token_id,
                expires_at: None,
            })
        }
    }
}

async fn device_approval_page(
    State(_state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    headers: HeaderMap,
) -> Response {
    let user_code = headers
        .get("x-user-code")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_default();
    let subject_html = html_escape(subject.0.as_deref().unwrap_or("(no session)"));
    let html = format!(
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>Approve MCP device</title>
<style>
body {{ font-family: -apple-system, system-ui, sans-serif; max-width: 480px; margin: 40px auto; padding: 0 16px; }}
input, button {{ font-size: 16px; padding: 8px; }}
input {{ width: 100%; box-sizing: border-box; letter-spacing: 2px; text-transform: uppercase; }}
label {{ display: block; margin-top: 12px; }}
#out {{ margin-top: 16px; padding: 12px; border: 1px solid #ccc; border-radius: 6px; display: none; }}
</style>
</head>
<body>
<h1>Approve MCP device</h1>
<p>Signed in as <code>{subject_html}</code>.</p>
<p>Enter the <strong>user code</strong> shown by the MCP client (format: <code>XXXX-XXXX</code>).</p>
<form id="form">
  <label>User code <input id="user_code" name="user_code" value="{prefill}" autofocus required></label>
  <label>Token label (optional) <input id="name" name="name" placeholder="e.g. claude-code on laptop"></label>
  <button type="submit">Approve</button>
</form>
<div id="out"></div>
<script>
document.getElementById('form').addEventListener('submit', async (e) => {{
  e.preventDefault();
  const out = document.getElementById('out');
  out.style.display = 'block';
  out.textContent = 'Approving...';
  const user_code = document.getElementById('user_code').value.trim().toUpperCase();
  const name = document.getElementById('name').value.trim() || null;
  try {{
    const res = await fetch('/auth/device/approve', {{
      method: 'POST',
      headers: {{ 'content-type': 'application/json' }},
      body: JSON.stringify({{ user_code, name }}),
    }});
    if (!res.ok) {{
      const err = await res.json().catch(() => ({{ error: res.statusText }}));
      out.textContent = 'Error: ' + (err.error || 'unknown');
      return;
    }}
    const data = await res.json();
    out.textContent = 'Approved. Token id ' + data.token_id + '. You can close this window; the CLI will pick up the token.';
  }} catch (err) {{
    out.textContent = 'Network error: ' + err;
  }}
}});
</script>
</body>
</html>
"#,
        prefill = html_escape(&user_code),
    );
    Html(html).into_response()
}

async fn verify_device(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<VerifyDeviceQuery>,
) -> Result<Json<VerifyDeviceResponse>, ApiError> {
    let user_code = query.user_code.trim().to_owned();
    if user_code.is_empty() {
        return Err(ApiError::BadRequest("user_code required".to_owned()));
    }
    let auth_store = state.auth_store.clone();
    tracing::debug!(user_code = %user_code, "verifying device auth request");

    let lookup_code = user_code.clone();
    let record =
        tokio::task::spawn_blocking(move || auth_store.find_device_auth_by_user_code(&lookup_code))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?
            .ok_or_else(|| {
                tracing::warn!(user_code = %user_code, "device auth request not found");
                ApiError::NotFound("user code not found".to_owned())
            })?;

    tracing::info!(
        user_code = %record.user_code,
        status = ?record.status,
        client_name = ?record.client_name,
        "device auth request verified"
    );

    Ok(Json(VerifyDeviceResponse {
        user_code: record.user_code,
        status: device_status_name(record.status).to_owned(),
        client_name: record.client_name,
        expires_at: record.expires_at,
    }))
}

#[derive(Debug, Deserialize)]
struct VerifyDeviceQuery {
    user_code: String,
}

async fn approve_device(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Json(body): Json<ApproveDeviceRequest>,
) -> Result<Json<ApproveDeviceResponse>, ApiError> {
    let user_code = body.user_code.trim().to_owned();
    if user_code.is_empty() {
        return Err(ApiError::BadRequest("user_code required".to_owned()));
    }

    let now = current_timestamp_millis()?;
    let auth_store = state.auth_store.clone();
    let record = {
        let auth_store = auth_store.clone();
        let user_code = user_code.clone();
        tokio::task::spawn_blocking(move || auth_store.find_device_auth_by_user_code(&user_code))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?
    }
    .ok_or_else(|| ApiError::NotFound("user code not found".to_owned()))?;

    if record.expires_at <= now {
        tracing::warn!(user_code = %user_code, "attempted to approve expired code");
        return Err(ApiError::BadRequest("code expired".to_owned()));
    }
    if !matches!(record.status, DeviceAuthStatus::Pending) {
        tracing::warn!(user_code = %user_code, status = ?record.status, "attempted to approve non-pending code");
        return Err(ApiError::BadRequest("code already used".to_owned()));
    }

    tracing::info!(
        user_code = %user_code,
        subject = ?subject.0,
        client_name = ?record.client_name,
        "approving device auth request"
    );

    let plaintext = mint_token_plaintext()?;
    let token_hash = hash_token(&plaintext);
    let token_id = Uuid::now_v7().to_string();
    let name = body
        .name
        .as_ref()
        .and_then(|value| non_empty(value.trim().to_owned()))
        .or_else(|| record.client_name.clone())
        .unwrap_or_else(|| "mcp device".to_owned());
    let expires_at = state
        .auth
        .mcp_token_ttl_days
        .map(|days| now + (days as i64) * 86_400_000);
    let new_token = NewMcpToken {
        id: token_id.clone(),
        token_hash,
        name,
        subject: subject.0.clone(),
        created_at: now,
        expires_at,
    };

    {
        let auth_store = auth_store.clone();
        tokio::task::spawn_blocking(move || auth_store.create_mcp_token(new_token))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
    }

    let subject_owned = subject.0.clone();
    let approved = {
        let auth_store = auth_store.clone();
        let user_code = user_code.clone();
        let token_id = token_id.clone();
        tokio::task::spawn_blocking(move || {
            auth_store.approve_device_auth(&user_code, &token_id, subject_owned.as_deref(), now)
        })
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?
    };

    if !approved {
        return Err(ApiError::BadRequest(
            "code expired or already used".to_owned(),
        ));
    }

    state
        .auth_store_cache()
        .store_pending_token(record.device_code.clone(), plaintext);

    Ok(Json(ApproveDeviceResponse {
        token_id,
        user_code,
    }))
}

async fn create_token(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, ApiError> {
    let name = non_empty(body.name.trim().to_owned()).unwrap_or_else(|| "manual token".to_owned());
    let now = current_timestamp_millis()?;
    let plaintext = mint_token_plaintext()?;
    let token_hash = hash_token(&plaintext);
    let token_id = Uuid::now_v7().to_string();
    let expires_at = state
        .auth
        .mcp_token_ttl_days
        .map(|days| now + (days as i64) * 86_400_000);
    let new_token = NewMcpToken {
        id: token_id.clone(),
        token_hash,
        name: name.clone(),
        subject: subject.0.clone(),
        created_at: now,
        expires_at,
    };

    let auth_store = state.auth_store.clone();
    tokio::task::spawn_blocking(move || auth_store.create_mcp_token(new_token))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;

    Ok(Json(CreateTokenResponse {
        token: plaintext,
        id: token_id,
        name,
        expires_at,
    }))
}

async fn list_tokens(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
) -> Result<Json<ListTokensResponse>, ApiError> {
    let auth_store = state.auth_store.clone();
    let subject_filter = subject.0.clone();
    let records =
        tokio::task::spawn_blocking(move || auth_store.list_mcp_tokens(subject_filter.as_deref()))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;

    Ok(Json(ListTokensResponse {
        tokens: records
            .into_iter()
            .map(|record| TokenSummary {
                id: record.id,
                name: record.name,
                subject: record.subject,
                created_at: record.created_at,
                last_used_at: record.last_used_at,
                expires_at: record.expires_at,
            })
            .collect(),
    }))
}

async fn revoke_token(
    State(state): State<AppState>,
    Extension(subject): Extension<SessionSubject>,
    Path(id): Path<String>,
) -> Result<Json<RevokeTokenResponse>, ApiError> {
    let auth_store = state.auth_store.clone();
    let subject_filter = subject.0.clone();
    let target_id = id.clone();
    let deleted = tokio::task::spawn_blocking(move || {
        auth_store.delete_mcp_token(&target_id, subject_filter.as_deref())
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    if !deleted {
        return Err(ApiError::NotFound("token not found".to_owned()));
    }

    Ok(Json(RevokeTokenResponse { id, deleted }))
}

fn device_status_name(status: DeviceAuthStatus) -> &'static str {
    match status {
        DeviceAuthStatus::Pending => "pending",
        DeviceAuthStatus::Approved => "approved",
        DeviceAuthStatus::Denied => "denied",
        DeviceAuthStatus::Expired => "expired",
    }
}

// ---- OAuth-MCP (RFC 8414 / 8628 / 7591) ---------------------------------

/// `GET /.well-known/oauth-protected-resource` — RFC 9728. Tells MCP HTTP
/// clients which authorization server backs the `/mcp` resource.
async fn oauth_protected_resource_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let base = verification_base_url(&state, &headers);
    Json(serde_json::json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base.clone()],
        "bearer_methods_supported": ["header"],
    }))
}

/// `GET /.well-known/oauth-authorization-server` — RFC 8414. Advertises the
/// device-authorization grant + the token + registration endpoints.
async fn oauth_authorization_server_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let base = verification_base_url(&state, &headers);
    Json(serde_json::json!({
        "issuer": base.clone(),
        "device_authorization_endpoint": format!("{base}/auth/device/code"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "grant_types_supported": [
            "urn:ietf:params:oauth:grant-type:device_code"
        ],
        "response_types_supported": [],
        "token_endpoint_auth_methods_supported": ["none"],
    }))
}

/// `POST /oauth/register` — RFC 7591 dynamic client registration. The
/// device flow doesn't actually need per-client secrets (Zitadel approval
/// is the real auth gate); we accept any metadata and return a static
/// `client_id` so MCP clients that insist on registering get a happy
/// response.
async fn oauth_register(
    body: Option<Json<serde_json::Value>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client_name = body
        .as_ref()
        .and_then(|Json(v)| v.get("client_name"))
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "mcp-client".to_owned());
    Ok(Json(serde_json::json!({
        "client_id": "rust-rag-mcp",
        "client_name": client_name,
        "token_endpoint_auth_method": "none",
        "grant_types": ["urn:ietf:params:oauth:grant-type:device_code"],
    })))
}

/// `POST /oauth/token` — RFC 6749 token endpoint, accepts
/// `application/x-www-form-urlencoded` with
/// `grant_type=urn:ietf:params:oauth:grant-type:device_code&device_code=...`.
/// Translates to the existing `device_token_inner` so the underlying
/// state machine stays in one place.
async fn oauth_token(
    State(state): State<AppState>,
    axum::extract::Form(form): axum::extract::Form<OAuthTokenForm>,
) -> Response {
    let want_grant = "urn:ietf:params:oauth:grant-type:device_code";
    if form.grant_type != want_grant {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unsupported_grant_type",
                "error_description": format!("only {want_grant} is supported"),
            })),
        )
            .into_response();
    }
    let device_code = match form.device_code {
        Some(s) if !s.is_empty() => s,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid_request", "error_description": "device_code is required"})),
            )
                .into_response();
        }
    };
    let req = DeviceTokenRequest { device_code };
    match device_token_inner(state, req).await {
        Ok(resp) => {
            let now = match current_timestamp_millis() {
                Ok(n) => n,
                Err(_) => 0,
            };
            // RFC 6749 prefers `expires_in` (seconds) over an absolute
            // `expires_at`. Compute it when available.
            let expires_in = resp
                .expires_at
                .filter(|&exp| exp > now)
                .map(|exp| ((exp - now) / 1000) as u64);
            let mut body = serde_json::json!({
                "access_token": resp.access_token,
                "token_type": "Bearer",
                "scope": "mcp",
            });
            if let Some(exp) = expires_in {
                body["expires_in"] = serde_json::Value::from(exp);
            }
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": err.as_str()})),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct OAuthTokenForm {
    pub grant_type: String,
    pub device_code: Option<String>,
    #[allow(dead_code)]
    pub client_id: Option<String>,
}

fn verification_base_url(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(base) = &state.auth.app_base_url {
        return base.trim_end_matches('/').to_owned();
    }

    // Try to guess from Host header
    if let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) {
        let proto = headers
            .get("x-forwarded-proto")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("http");
        return format!("{}://{}", proto, host);
    }

    "".to_owned()
}

fn random_base64url(bytes: usize) -> Result<String, ApiError> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf)
        .map_err(|error| {
            tracing::error!(error = %error, "getrandom failed in random_base64url");
            ApiError::Internal(anyhow::anyhow!("getrandom failed: {error}"))
        })?;
    Ok(URL_SAFE_NO_PAD.encode(&buf))
}

fn random_user_code() -> Result<String, ApiError> {
    // Crockford-ish alphabet: no O/0/I/1/L to avoid transcription errors.
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf)
        .map_err(|error| {
            tracing::error!(error = %error, "getrandom failed in random_user_code");
            ApiError::Internal(anyhow::anyhow!("getrandom failed: {error}"))
        })?;
    let chars: Vec<char> = buf
        .iter()
        .map(|byte| ALPHABET[(*byte as usize) % ALPHABET.len()] as char)
        .collect();
    Ok(format!(
        "{}{}{}{}-{}{}{}{}",
        chars[0], chars[1], chars[2], chars[3], chars[4], chars[5], chars[6], chars[7]
    ))
}

fn mint_token_plaintext() -> Result<String, ApiError> {
    Ok(format!("{MCP_TOKEN_PREFIX}{}", random_base64url(32)?))
}

pub(super) fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One-shot, in-memory cache for token plaintext that bridges the approve
/// handler (which mints the token) and the device_token handler (which
/// returns it to the polling client). Keyed by device_code, single read.
#[derive(Default)]
pub(super) struct PendingTokenCache {
    inner: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl PendingTokenCache {
    pub fn store_pending_token(&self, device_code: String, plaintext: String) {
        self.inner
            .lock()
            .expect("pending-token cache poisoned")
            .insert(device_code, plaintext);
    }

    pub fn take_pending_token(&self, device_code: &str) -> Option<String> {
        self.inner
            .lock()
            .expect("pending-token cache poisoned")
            .remove(device_code)
    }
}

impl AppState {
    pub(super) fn auth_store_cache(&self) -> Arc<PendingTokenCache> {
        self.pending_tokens.clone()
    }
}

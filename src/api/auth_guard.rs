use axum::{
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

use super::auth::{self, SessionSubject};
use super::error::{ApiError, current_timestamp_millis};
use super::state::AppState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AuthKind {
    Disabled,
    ApiKey,
    McpToken,
    SessionCookie,
}

impl AuthKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::ApiKey => "api_key",
            Self::McpToken => "mcp_token",
            Self::SessionCookie => "session_cookie",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RequestAuthContext {
    pub(crate) subject: Option<String>,
    pub(crate) kind: AuthKind,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Claims {
    pub(crate) sub: String,
    pub(crate) name: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) preferred_username: Option<String>,
    pub(crate) exp: usize,
}

pub(crate) async fn require_api_key(
    State(state): State<AppState>,
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        "checking api key for request"
    );
    if !state.auth.is_enabled() {
        request.extensions_mut().insert(SessionSubject(None));
        request.extensions_mut().insert(RequestAuthContext {
            subject: None,
            kind: AuthKind::Disabled,
        });
        return Ok(next.run(request).await);
    }

    let provided = request
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            request
                .headers()
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    if let Some(ref key) = provided {
        if state.auth.matches_api_key(key) {
            request.extensions_mut().insert(SessionSubject(None));
            request.extensions_mut().insert(RequestAuthContext {
                subject: None,
                kind: AuthKind::ApiKey,
            });
            return Ok(next.run(request).await);
        }

        if key.starts_with(auth::MCP_TOKEN_PREFIX) {
            let hash = auth::hash_token(key);
            let auth_store = state.auth_store.clone();
            let record =
                tokio::task::spawn_blocking(move || auth_store.find_mcp_token_by_hash(&hash))
                    .await
                    .map_err(ApiError::TaskJoin)?
                    .map_err(ApiError::Internal)?;
            if let Some(record) = record {
                let now = current_timestamp_millis()?;
                if record
                    .expires_at
                    .map(|expiry| expiry <= now)
                    .unwrap_or(false)
                {
                    tracing::warn!(token_id = %record.id, "rejecting expired MCP token");
                } else {
                    let touch_store = state.auth_store.clone();
                    let touch_id = record.id.clone();
                    tokio::task::spawn_blocking(move || {
                        if let Err(error) = touch_store.touch_mcp_token(&touch_id, now) {
                            tracing::warn!(error = %error, "failed to update token last_used_at");
                        }
                    });
                    tracing::debug!(token_id = %record.id, "authorized via MCP token");
                    let subject = record.subject.clone();
                    request
                        .extensions_mut()
                        .insert(SessionSubject(subject.clone()));
                    request.extensions_mut().insert(RequestAuthContext {
                        subject,
                        kind: AuthKind::McpToken,
                    });
                    return Ok(next.run(request).await);
                }
            }
        }
    }

    if let Some(secret) = state.auth.session_secret.as_deref() {
        if let Some(cookies) = request
            .headers()
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
        {
            for cookie in cookies.split(';') {
                let mut parts = cookie.trim().splitn(2, '=');
                if let (Some(name), Some(value)) = (parts.next(), parts.next()) {
                    if name == "rag_session" {
                        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
                        validation.validate_aud = false; // Token has no aud claim

                        match decode::<Claims>(
                            value,
                            &DecodingKey::from_secret(secret.as_bytes()),
                            &validation,
                        ) {
                            Ok(token_data) => {
                                tracing::debug!("authorized via session cookie");
                                let subject = token_data.claims.sub;
                                request
                                    .extensions_mut()
                                    .insert(SessionSubject(Some(subject.clone())));
                                request.extensions_mut().insert(RequestAuthContext {
                                    subject: Some(subject),
                                    kind: AuthKind::SessionCookie,
                                });
                                return Ok(next.run(request).await);
                            }
                            Err(err) => {
                                tracing::debug!(error = %err, "failed to decode session cookie");
                            }
                        }
                    }
                }
            }
        } else {
            tracing::debug!("no cookie header found in request");
        }
    } else {
        tracing::warn!(
            "AUTH_SESSION_SECRET not configured in backend - session cookies will be ignored"
        );
    }

    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        has_x_api_key = provided.is_some(),
        has_cookies = request.headers().contains_key(axum::http::header::COOKIE),
        "unauthorized request: no valid credential found"
    );

    // MCP HTTP clients do auto-discovery via the WWW-Authenticate header on
    // a 401 to the resource (`/mcp`). Without this, Claude Code etc. just
    // give up. The `resource_metadata` URL points the client at our
    // `.well-known/oauth-protected-resource` document, which in turn names
    // the authorization server (us).
    let path = request.uri().path();
    if path == "/mcp" || path.starts_with("/mcp/") {
        let base = request
            .headers()
            .get(axum::http::header::HOST)
            .and_then(|h| h.to_str().ok())
            .map(|host| {
                let proto = request
                    .headers()
                    .get("x-forwarded-proto")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("https");
                format!("{}://{}", proto, host)
            })
            .unwrap_or_else(|| state.auth.app_base_url.clone().unwrap_or_default());
        let resource_metadata = format!(
            "{}/.well-known/oauth-protected-resource",
            base.trim_end_matches('/')
        );
        let www_auth = format!(
            "Bearer realm=\"rust-rag-mcp\", resource_metadata=\"{}\"",
            resource_metadata
        );
        return Ok((
            StatusCode::UNAUTHORIZED,
            [
                (axum::http::header::WWW_AUTHENTICATE, www_auth),
                (
                    axum::http::header::CONTENT_TYPE,
                    "application/json".to_owned(),
                ),
            ],
            "{\"error\":\"unauthorized\"}",
        )
            .into_response());
    }

    Err(ApiError::Unauthorized(
        "missing x-api-key header, bearer token or valid session cookie".to_owned(),
    ))
}

pub(crate) fn is_admin_subject(subject: Option<&str>) -> bool {
    let Some(subject) = subject else { return false };
    let raw = match std::env::var("RAG_ADMIN_SUBJECTS") {
        Ok(v) => v,
        Err(_) => return false,
    };
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|allowed| allowed == subject)
}

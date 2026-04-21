use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct LoginOptions {
    pub base_url: String,
    pub token_path: PathBuf,
    pub client_name: Option<String>,
    pub timeout: Duration,
}

#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    client_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Serialize)]
struct DeviceTokenRequest<'a> {
    device_code: &'a str,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
    token_id: String,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenError {
    error: String,
}

pub async fn run(options: LoginOptions) -> Result<()> {
    let http = Client::builder()
        .timeout(options.timeout)
        .build()
        .context("building HTTP client")?;
    let base = options.base_url.trim_end_matches('/');

    let code: DeviceCodeResponse = http
        .post(format!("{base}/auth/device/code"))
        .json(&DeviceCodeRequest {
            client_name: options.client_name.clone(),
        })
        .send()
        .await
        .context("requesting device code")?
        .error_for_status()
        .context("device code request failed")?
        .json()
        .await
        .context("decoding device code response")?;

    eprintln!();
    eprintln!("Open this URL to approve:");
    eprintln!("  {}", code.verification_uri_complete);
    eprintln!();
    eprintln!("Or go to {} and enter code:", code.verification_uri);
    eprintln!("  {}", code.user_code);
    eprintln!();
    eprintln!(
        "Waiting for approval (expires in {}s, polling every {}s)...",
        code.expires_in,
        code.interval.max(1)
    );

    let poll_interval = Duration::from_secs(code.interval.max(1));
    let deadline = std::time::Instant::now() + Duration::from_secs(code.expires_in);

    loop {
        if std::time::Instant::now() >= deadline {
            bail!("device code expired before approval");
        }
        tokio::time::sleep(poll_interval).await;

        let response = http
            .post(format!("{base}/auth/device/token"))
            .json(&DeviceTokenRequest {
                device_code: &code.device_code,
            })
            .send()
            .await
            .context("polling for token")?;

        match response.status() {
            StatusCode::OK => {
                let token: DeviceTokenResponse =
                    response.json().await.context("decoding token response")?;
                write_token(&options.token_path, &token.access_token)?;
                eprintln!();
                eprintln!(
                    "Approved. Token id {} written to {}",
                    token.token_id,
                    options.token_path.display()
                );
                return Ok(());
            }
            StatusCode::BAD_REQUEST => {
                let body: DeviceTokenError =
                    response.json().await.context("decoding error response")?;
                match body.error.as_str() {
                    "authorization_pending" | "slow_down" => continue,
                    other => bail!("device login failed: {other}"),
                }
            }
            status => {
                let body = response.text().await.unwrap_or_default();
                bail!("unexpected {status} from device token endpoint: {body}");
            }
        }
    }
}

fn write_token(path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating token directory {}", parent.display()))?;
    }
    std::fs::write(path, token).with_context(|| format!("writing token to {}", path.display()))?;
    set_secure_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_secure_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("reading metadata for {}", path.display()))?
        .permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("chmod 600 {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_secure_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

pub fn default_token_path() -> Result<PathBuf> {
    if let Some(explicit) = std::env::var_os("RAG_MCP_TOKEN_PATH") {
        return Ok(PathBuf::from(explicit));
    }
    let base = if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        return Err(anyhow!("cannot resolve default token path: HOME unset"));
    };
    Ok(base.join("rust-rag").join("mcp-token"))
}

pub fn read_token_from_file(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

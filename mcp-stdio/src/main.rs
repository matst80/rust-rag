use anyhow::{Result, bail};
use mcp_stdio::{
    BridgeConfig, RustRagHttpClient, RustRagMcpServer,
    client::HttpClientConfig,
    login::{LoginOptions, default_token_path, read_token_from_file, run as run_login},
    server::BridgeServerInfo,
};
use rmcp::{ServiceExt, transport::io::stdio};
use std::{path::PathBuf, time::Duration};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mcp_stdio=info,rmcp=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("login")) {
        return login(&args[1..]).await;
    }
    if matches!(
        args.first().map(String::as_str),
        Some("--help") | Some("-h") | Some("help")
    ) {
        print_usage();
        return Ok(());
    }

    let mut config = BridgeConfig::from_env()?;
    if config.auth_bearer.is_none() {
        if let Ok(path) = default_token_path() {
            if let Some(token) = read_token_from_file(&path) {
                info!(path = %path.display(), "loaded MCP token from file");
                config.auth_bearer = Some(token);
            }
        }
    }

    let client = RustRagHttpClient::new(HttpClientConfig {
        base_url: config.api_base_url.clone(),
        timeout: config.request_timeout,
        auth_bearer: config.auth_bearer.clone(),
        headers: config.headers.clone(),
    })?;
    let server = RustRagMcpServer::new(
        client,
        &config.enabled_groups,
        BridgeServerInfo {
            name: config.server_name.clone(),
            version: config.server_version.clone(),
            instructions: config.server_instructions.clone(),
        },
        config.search_format,
    );

    info!(
        api_base_url = %config.api_base_url,
        enabled_groups = %config
            .enabled_groups
            .iter()
            .map(|group| group.as_str())
            .collect::<Vec<_>>()
            .join(","),
        "starting rust-rag MCP stdio bridge"
    );

    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}

fn print_usage() {
    eprintln!("rust-rag mcp-stdio");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  mcp-stdio                     run the stdio bridge (default)");
    eprintln!("  mcp-stdio login [flags]       obtain an MCP token via device flow");
    eprintln!();
    eprintln!("LOGIN FLAGS:");
    eprintln!("  --base-url <url>              override RAG_MCP_API_BASE_URL");
    eprintln!("  --token-path <path>           where to store the token");
    eprintln!("                                (default: $XDG_CONFIG_HOME/rust-rag/mcp-token)");
    eprintln!("  --client-name <name>          label to attach to the token");
}

async fn login(args: &[String]) -> Result<()> {
    let base_url_env = std::env::var("RAG_MCP_API_BASE_URL").ok();
    let mut base_url: Option<String> = base_url_env;
    let mut token_path: Option<PathBuf> = None;
    let mut client_name: Option<String> = Some(
        std::env::var("RAG_MCP_CLIENT_NAME")
            .ok()
            .unwrap_or_else(|| "mcp-stdio".to_owned()),
    );

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--base-url" => {
                base_url = iter
                    .next()
                    .map(|value| value.to_owned())
                    .or_else(|| None)
                    .or(base_url);
            }
            "--token-path" => {
                let Some(value) = iter.next() else {
                    bail!("--token-path requires a value");
                };
                token_path = Some(PathBuf::from(value));
            }
            "--client-name" => {
                let Some(value) = iter.next() else {
                    bail!("--client-name requires a value");
                };
                client_name = Some(value.to_owned());
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => bail!("unknown login argument: {other}"),
        }
    }

    let base_url = base_url.unwrap_or_else(|| "https://rag.k6n.net".to_owned());
    let token_path = match token_path {
        Some(path) => path,
        None => default_token_path()?,
    };

    run_login(LoginOptions {
        base_url,
        token_path,
        client_name,
        timeout: Duration::from_secs(30),
    })
    .await
}

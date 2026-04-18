use anyhow::Result;
use mcp_stdio::{
    BridgeConfig, RustRagHttpClient, RustRagMcpServer,
    client::HttpClientConfig,
    server::BridgeServerInfo,
};
use rmcp::{ServiceExt, transport::io::stdio};
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

    let config = BridgeConfig::from_env()?;
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
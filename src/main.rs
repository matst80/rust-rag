use anyhow::Result;
use std::sync::Arc;
use tokio::signal;
use tracing::info;

use rust_rag::{
    api::{AppState, EmbedderHandle},
    build_app,
    config::AppConfig,
    db::{AuthStore, SqliteVectorStore, VectorStore},
    embedding::{Embedder, EmbeddingService},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rust_rag=info,axum=info,tower_http=info".into()),
        )
        .init();

    let config = AppConfig::from_env()?;
    println!("rust-rag booting");
    println!("config: binding to http://{}", config.bind_address());
    println!("config: sqlite db {}", config.db_path);
    println!(
        "config: graph enabled={} build_on_startup={} k={} max_distance={} cross_source={}",
        config.graph_enabled,
        config.graph_build_on_startup,
        config.graph_similarity_top_k,
        config.graph_similarity_max_distance,
        config.graph_cross_source
    );
    println!("loading sqlite store");
    println!(
        "config: auth enabled={} (frontend_key={}, session_secret={}, api_keys={})",
        config.auth.enabled,
        config.auth.frontend_api_key.is_some(),
        config.auth.session_secret.is_some(),
        config.auth.api_keys.len()
    );
    let store = Arc::new(SqliteVectorStore::connect(
        &config.db_path,
        config.embedding_dimension,
        config.graph_config(),
    )?);

    if config.graph_enabled && config.graph_build_on_startup {
        println!("rebuilding similarity graph");
        let rebuilt = store.rebuild_similarity_graph()?;
        println!("similarity graph rebuilt with {rebuilt} edges");
    }

    let store_service: Arc<dyn VectorStore> = store.clone();
    let auth_store: Arc<dyn AuthStore> = store.clone();
    let embedder_handle = Arc::new(EmbedderHandle::loading());
    let state = AppState::new(
        embedder_handle.clone(),
        store_service,
        auth_store,
        config.auth.clone(),
        config.openai_chat.clone(),
    );
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(config.bind_address()).await?;
    let local_addr = listener.local_addr()?;
    println!("rust-rag listening on http://{local_addr}");
    info!("rust-rag listening on http://{local_addr}");

    let model_path = config.model_path.clone();
    let tokenizer_path = config.tokenizer_path.clone();
    let ort_dylib_path = config.ort_dylib_path.clone();
    let intra_threads = config.intra_threads;
    tokio::task::spawn_blocking(move || {
        println!("loading embedding model from {}", model_path.display());
        match Embedder::from_paths(
            &model_path,
            &tokenizer_path,
            intra_threads,
            ort_dylib_path.as_deref(),
        ) {
            Ok(embedder) => {
                println!("embedding model loaded");
                let embedder_service: Arc<dyn EmbeddingService> = Arc::new(embedder);
                embedder_handle.mark_ready(embedder_service);
            }
            Err(error) => {
                eprintln!("failed to load embedding model: {error}");
                embedder_handle.mark_failed(error.to_string());
            }
        }
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    store.close()?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    info!("shutdown signal received");
}

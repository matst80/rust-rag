use anyhow::Result;
use opentelemetry::{KeyValue, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    logs::LoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    runtime,
    trace::TracerProvider,
};
use std::sync::Arc;
use tokio::signal;
use tracing::info;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use rust_rag::{
    api::{AppState, EmbedderHandle},
    build_app,
    config::AppConfig,
    db::{AuthStore, MessageStore, SqliteVectorStore, UserMemoryStore, VectorStore},
    embedding::{Embedder, EmbeddingService},
    manager, ontology,
};

/// Bundle of OTel providers built from env. Caller shuts each down at exit.
struct OtelProviders {
    tracer: TracerProvider,
    logger: LoggerProvider,
    meter: SdkMeterProvider,
}

/// Build OTLP-gRPC trace + log + metric pipelines if `RAG_OTEL_ENABLED=true`.
fn init_otel() -> Result<Option<OtelProviders>> {
    let enabled = std::env::var("RAG_OTEL_ENABLED")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !enabled {
        return Ok(None);
    }
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_owned());
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "rust-rag".to_owned());
    let resource = Resource::new([KeyValue::new("service.name", service_name.clone())]);

    let span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()?;
    let tracer = TracerProvider::builder()
        .with_batch_exporter(span_exporter, runtime::Tokio)
        .with_resource(resource.clone())
        .build();

    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()?;
    let logger = LoggerProvider::builder()
        .with_batch_exporter(log_exporter, runtime::Tokio)
        .with_resource(resource.clone())
        .build();

    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()?;
    let reader = PeriodicReader::builder(metric_exporter, runtime::Tokio)
        .with_interval(std::time::Duration::from_secs(15))
        .build();
    let meter = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    opentelemetry::global::set_meter_provider(meter.clone());

    eprintln!("otel: traces+logs+metrics → {endpoint} (service={service_name})");
    Ok(Some(OtelProviders { tracer, logger, meter }))
}

#[tokio::main]
async fn main() -> Result<()> {
    // fmt subscriber stays terse: drop tower_http's per-request DEBUG events
    // and normal axum chatter from console output. OTel layer (below) gets
    // its own filter that opens these up so request spans actually export.
    let fmt_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "rust_rag=info,axum=info,tower_http=info".into());
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_filter(fmt_filter);

    let otel_provider = init_otel()?;
    if let Some(providers) = otel_provider.as_ref() {
        // Request spans live at DEBUG inside tower_http; export them so
        // every HTTP call shows up as a root span at the collector. Keep
        // rust-rag at info to avoid drowning the trace stream.
        let otel_filter = std::env::var("RAG_OTEL_FILTER")
            .ok()
            .and_then(|v| v.parse::<tracing_subscriber::EnvFilter>().ok())
            .unwrap_or_else(|| {
                tracing_subscriber::EnvFilter::new(
                    "rust_rag=info,axum=info,tower_http=debug",
                )
            });
        let log_filter = std::env::var("RAG_OTEL_LOG_FILTER")
            .ok()
            .and_then(|v| v.parse::<tracing_subscriber::EnvFilter>().ok())
            .unwrap_or_else(|| {
                tracing_subscriber::EnvFilter::new("rust_rag=info,axum=info,tower_http=info")
            });
        let tracer = providers.tracer.tracer("rust-rag");
        let otel_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(otel_filter);
        let log_layer = OpenTelemetryTracingBridge::new(&providers.logger).with_filter(log_filter);
        tracing_subscriber::registry()
            .with(fmt_layer)
            .with(otel_layer)
            .with(log_layer)
            .init();
    } else {
        tracing_subscriber::registry().with(fmt_layer).init();
    }

    if otel_provider.is_some() {
        let meter = opentelemetry::global::meter("rust-rag");
        let started = std::time::Instant::now();
        let uptime = meter
            .u64_observable_gauge("rust_rag.uptime_seconds")
            .with_description("Seconds since process start")
            .with_callback(move |obs| obs.observe(started.elapsed().as_secs(), &[]))
            .build();
        // Leak so the callback registration outlives this scope.
        Box::leak(Box::new(uptime));
    }

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

    // VectorStore + MessageStore: route to Postgres when RAG_DATABASE_URL is
    // set. Auth + user_memory still on SQLite — those tables port in a
    // follow-up slice. The Postgres + SQLite stores coexist during the
    // cutover window.
    let pg_store = if let Some(url) = &config.database_url {
        let pg_pool = rust_rag::db::postgres::connect(url, 10).await?;
        info!("postgres: connected, vector/message/auth/user-memory routed to {url}");
        let pg = Arc::new(rust_rag::db::postgres::PostgresVectorStore::new(
            pg_pool,
            tokio::runtime::Handle::current(),
            config.graph_config(),
        ));
        if config.graph_enabled && config.graph_build_on_startup {
            println!("rebuilding similarity graph (postgres)");
            let pg_clone = pg.clone();
            let rebuilt = tokio::task::spawn_blocking(move || {
                pg_clone.rebuild_similarity_graph()
            })
            .await??;
            println!("similarity graph rebuilt with {rebuilt} edges");
        }
        Some(pg)
    } else {
        if config.graph_enabled && config.graph_build_on_startup {
            println!("rebuilding similarity graph");
            let rebuilt = store.rebuild_similarity_graph()?;
            println!("similarity graph rebuilt with {rebuilt} edges");
        }
        None
    };
    let store_service: Arc<dyn VectorStore> = match &pg_store {
        Some(pg) => pg.clone(),
        None => store.clone(),
    };
    match rust_rag::validation::seed_bundled_schemas(store_service.as_ref()) {
        Ok(n) if n > 0 => info!("seeded {n} bundled schema(s)"),
        Ok(_) => {}
        Err(e) => tracing::warn!(?e, "schema seeding failed"),
    }
    let message_store: Arc<dyn MessageStore> = match &pg_store {
        Some(pg) => pg.clone(),
        None => store.clone(),
    };
    let auth_store: Arc<dyn AuthStore> = match &pg_store {
        Some(pg) => pg.clone(),
        None => store.clone(),
    };
    let user_memory: Arc<dyn UserMemoryStore> = match &pg_store {
        Some(pg) => pg.clone(),
        None => store.clone(),
    };
    let embedder_handle = Arc::new(EmbedderHandle::loading());
    let state = AppState::new(
        embedder_handle.clone(),
        store_service.clone(),
        auth_store,
        user_memory,
        message_store,
        config.auth.clone(),
        config.openai_chat.clone(),
        config.multimodal.clone(),
        config.upload_path.clone(),
        config.chunking.clone(),
    )
    .with_manager(config.manager.clone())
    .with_analysis(config.analysis.clone());

    // Build the markdown chunker from the embedder's tokenizer so chunk size
    // is measured in real model tokens. Only enabled when running against
    // Postgres — the SQLite store doesn't have a chunks table.
    let md_chunker = if config.database_url.is_some() {
        let max_tokens = std::env::var("RAG_CHUNK_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500_usize);
        let overlap_tokens = std::env::var("RAG_CHUNK_OVERLAP_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50_usize);
        let tokenizer = tokenizers::Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| anyhow::anyhow!("loading tokenizer for chunker: {e}"))?;
        Some(Arc::new(rust_rag::chunking_md::MarkdownChunker::new(
            tokenizer,
            max_tokens,
            overlap_tokens,
        )?))
    } else {
        None
    };

    info!(
        "md_chunker enabled = {} (max={} tokens, overlap={} tokens)",
        md_chunker.is_some(),
        std::env::var("RAG_CHUNK_MAX_TOKENS").unwrap_or_else(|_| "500".into()),
        std::env::var("RAG_CHUNK_OVERLAP_TOKENS").unwrap_or_else(|_| "50".into()),
    );

    // Optional cross-encoder reranker. Loaded only when explicitly enabled —
    // it's a second ORT session that takes ~1.1 GB fp16 GPU and adds
    // ~50–150 ms / batch latency. Both env knobs must be set.
    let reranker_enabled = std::env::var("RAG_RERANKER_ENABLED")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let reranker: Option<std::sync::Arc<dyn rust_rag::reranker::Reranker>> = if reranker_enabled {
        let model_path = std::env::var_os("RAG_RERANKER_MODEL_PATH")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!(
                "RAG_RERANKER_ENABLED=true but RAG_RERANKER_MODEL_PATH is unset"
            ))?;
        let tokenizer_path = std::env::var_os("RAG_RERANKER_TOKENIZER_PATH")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!(
                "RAG_RERANKER_ENABLED=true but RAG_RERANKER_TOKENIZER_PATH is unset"
            ))?;
        let max_length = std::env::var("RAG_RERANKER_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(512_usize);
        let intra_threads = std::env::var("RAG_RERANKER_INTRA_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2_usize);
        let dylib = std::env::var_os("ORT_DYLIB_PATH").map(std::path::PathBuf::from);
        let dylib_ref = dylib.as_deref();
        let backend = rust_rag::reranker::OrtReranker::from_paths(
            &model_path,
            &tokenizer_path,
            intra_threads,
            max_length,
            dylib_ref,
        )?;
        info!("reranker loaded from {}", model_path.display());
        Some(std::sync::Arc::new(backend))
    } else {
        None
    };
    let reranker_top_n = std::env::var("RAG_RERANKER_TOP_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_usize);

    let acp_registry = rust_rag::acp_ws::AcpWsRegistry::new(&config.acp_ws);
    let mut state = state;
    state.md_chunker = md_chunker;
    state.acp_ws = Some(acp_registry.clone());
    if let Some(r) = reranker {
        state.reranker = Some(r);
        state.reranker_top_n = reranker_top_n.max(1);
    }

    // ACP instance discovery. Default: mDNS browse `_acp-ws._tcp` on the LAN.
    // When the service runs in k8s on a subnet that can't see the user's
    // network (multicast doesn't traverse), set RAG_ACP_DISCOVERY_MODE=http
    // so clients register over HTTP instead.
    //
    // Every discovered/registered instance gets its own worker in the
    // registry, so multiple daemons can be served concurrently to multiple
    // browser tabs.
    let acp_token = config.acp_ws.token.clone();
    let registry_for_register = acp_registry.clone();
    let registry_for_unregister = acp_registry.clone();
    let hooks = rust_rag::acp_discovery::DiscoveryHooks {
        on_register: std::sync::Arc::new(move |instance| {
            let registry = registry_for_register.clone();
            let name = instance.name.clone();
            let url = instance.url.clone();
            let token = acp_token.clone();
            tokio::spawn(async move {
                registry.register(name, url, token).await;
            });
        }),
        on_unregister: std::sync::Arc::new(move |name| {
            let registry = registry_for_unregister.clone();
            let name = name.to_owned();
            tokio::spawn(async move {
                registry.unregister(&name).await;
            });
        }),
        on_select: std::sync::Arc::new(|_| {}),
    };
    let discovery_mode = std::env::var("RAG_ACP_DISCOVERY_MODE")
        .unwrap_or_else(|_| "mdns".to_owned())
        .to_lowercase();
    let discovery = match discovery_mode.as_str() {
        "http" | "register" | "off" => Some(rust_rag::acp_discovery::spawn_http_only(hooks)),
        _ => rust_rag::acp_discovery::spawn(hooks),
    };
    state.acp_discovery = discovery;

    let app = build_app(state.clone());

    let listener = tokio::net::TcpListener::bind(config.bind_address()).await?;
    let local_addr = listener.local_addr()?;
    println!("rust-rag listening on http://{local_addr}");
    info!("rust-rag listening on http://{local_addr}");

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut manager_handle = None;
    if config.manager.enabled {
        let manager_state = state.clone();
        let manager_config = config.manager.clone();
        let manager_shutdown = shutdown_rx.clone();
        manager_handle = Some(tokio::spawn(manager::run_manager_worker(
            manager_state,
            manager_config,
            manager_shutdown,
        )));
    }

    let mut ontology_handle = None;
    if config.ontology.enabled {
        ontology_handle = Some(tokio::spawn(ontology::run_ontology_worker(
            store_service.clone(),
            embedder_handle.clone(),
            reqwest::Client::new(),
            config.openai_chat.clone(),
            config.ontology.clone(),
            shutdown_rx,
        )));
    }

    let model_path = config.model_path.clone();
    let tokenizer_path = config.tokenizer_path.clone();
    let ort_dylib_path = config.ort_dylib_path.clone();
    let intra_threads = config.intra_threads;
    let pooling = config.embedding_pooling;
    tokio::task::spawn_blocking(move || {
        println!("loading embedding model from {}", model_path.display());
        match Embedder::from_paths(
            &model_path,
            &tokenizer_path,
            intra_threads,
            ort_dylib_path.as_deref(),
        ) {
            Ok(embedder) => {
                println!("embedding model loaded (pooling={pooling:?})");
                let embedder_service: Arc<dyn EmbeddingService> =
                    Arc::new(embedder.with_pooling(pooling));
                embedder_handle.mark_ready(embedder_service);
            }
            Err(error) => {
                eprintln!("failed to load embedding model: {error}");
                embedder_handle.mark_failed(error.to_string());
            }
        }
    });

    let state_for_shutdown = state.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            info!("shutdown signal received, notifying components");
            let _ = shutdown_tx.send(true);
            state_for_shutdown.message_notify.notify_waiters();
        })
        .await?;

    if let Some(handle) = ontology_handle {
        info!("waiting for ontology worker to finish");
        let _ = handle.await;
    }

    if let Some(handle) = manager_handle {
        info!("waiting for manager worker to finish");
        let _ = handle.await;
    }

    info!("closing sqlite store");
    store.close()?;

    if let Some(providers) = otel_provider {
        info!("flushing otel exporters");
        let _ = providers.tracer.shutdown();
        let _ = providers.logger.shutdown();
        let _ = providers.meter.shutdown();
    }
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

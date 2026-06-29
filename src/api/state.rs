use crate::{
    config::{
        AnalysisConfig, AuthConfig, ChunkingConfig, DreamingConfig, GoogleOAuthConfig,
        ManagerConfig, MultimodalConfig, OntologyConfig, OpenAiChatConfig, WebPushConfig,
    },
    crypto::EncryptionKey,
    db::{
        AuthStore, ChannelSummary, MessageQuery, MessageRecord, MessageStore, MessageUpdate,
        NewMessage, NewUserEvent, OAuthCredsStore, PushStore, UserMemoryStore, VectorStore,
    },
    embedding::EmbeddingService,
};
use axum::{Json, http::StatusCode};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use super::auth;
use super::error::ApiError;
use super::health::HealthResponse;
use super::presence::PresenceTracker;
use super::tombstones::TombstoneTracker;

#[derive(Clone)]
pub struct AppState {
    pub embedder: Arc<EmbedderHandle>,
    pub store: Arc<dyn VectorStore>,
    pub auth_store: Arc<dyn AuthStore>,
    pub user_memory: Arc<dyn UserMemoryStore>,
    pub messages: Arc<dyn MessageStore>,
    pub manager_runtime: Option<Arc<ManagerConfig>>,
    pub acp_ws: Option<Arc<crate::acp_ws::AcpWsRegistry>>,
    pub acp_discovery: Option<crate::acp_discovery::AcpDiscoveryHandle>,
    pub presence: Arc<PresenceTracker>,
    pub tombstones: Arc<TombstoneTracker>,
    pub message_notify: Arc<tokio::sync::Notify>,
    /// Fan-out of every successfully persisted message. `wait_for_message`
    /// (MCP) and any future per-message subscribers can filter without
    /// re-querying the DB. Capacity is generous; lagging consumers see
    /// `RecvError::Lagged` and should keep recv-ing.
    pub message_broadcast: Arc<tokio::sync::broadcast::Sender<MessageRecord>>,
    pub auth: Arc<AuthConfig>,
    pub openai_chat: Arc<OpenAiChatConfig>,
    pub multimodal: Arc<MultimodalConfig>,
    pub upload_path: Arc<String>,
    pub chunking: Arc<ChunkingConfig>,
    /// Token + markdown-aware chunker. Set when bge-m3 is loaded; absent in
    /// pure-SQLite mode where the legacy char-based chunker is still used.
    pub md_chunker: Option<Arc<crate::chunking_md::MarkdownChunker>>,
    /// Cross-encoder reranker; `None` when `RAG_RERANKER_ENABLED` is unset.
    /// When present, hybrid search pulls the top-N candidate pool from the
    /// store and re-scores it with this model before truncating to top-K.
    pub reranker: Option<Arc<dyn crate::reranker::Reranker>>,
    /// Pool size for the reranker candidate stage. Ignored when
    /// `reranker` is None.
    pub reranker_top_n: usize,
    pub http_client: reqwest::Client,
    pub multimodal_client: reqwest::Client,
    pub analysis: Arc<AnalysisConfig>,
    pub dreaming: Arc<DreamingConfig>,
    pub ontology: Arc<OntologyConfig>,
    /// Effective LLM endpoint for the ontology worker. Prefers
    /// `RAG_ANALYSIS_*` when fully configured (so analysis + ontology share
    /// the typed-output-friendly model); otherwise falls back to
    /// `RAG_OPENAI_*`. `None` when neither is configured.
    pub ontology_llm: Arc<OpenAiChatConfig>,
    /// Compiled-schema cache for typed-entry validation. Lives for the
    /// lifetime of the process; invalidated on schema upsert/delete.
    pub schema_cache: Arc<crate::validation::SchemaCache>,
    pub(in crate::api) pending_tokens: Arc<auth::PendingTokenCache>,
    /// Per-user encrypted OAuth credentials for external integrations
    /// (Google, etc.). `None` when no integrations backend is wired in
    /// (e.g. minimal test fixtures).
    pub oauth_creds: Option<Arc<dyn OAuthCredsStore>>,
    pub google_oauth: Arc<GoogleOAuthConfig>,
    /// Master AES-GCM key used by `oauth_creds`. `None` when
    /// `OAUTH_TOKEN_ENC_KEY` is not configured — integrations endpoints
    /// will refuse to start the OAuth flow in that case.
    pub oauth_token_key: Option<Arc<EncryptionKey>>,
    /// Per-subject Web Push subscriptions backend. `None` in minimal test
    /// fixtures or when push is intentionally disabled.
    pub push: Option<Arc<dyn PushStore>>,
    pub web_push: Arc<WebPushConfig>,
    pub whisper: Arc<crate::config::WhisperConfig>,
    pub projection_worker: Arc<crate::projection::ProjectionWorker>,
    pub cms_runtime: Arc<crate::cms::CmsRuntime>,
}

impl AppState {
    pub fn new(
        embedder: Arc<EmbedderHandle>,
        store: Arc<dyn VectorStore>,
        auth_store: Arc<dyn AuthStore>,
        user_memory: Arc<dyn UserMemoryStore>,
        messages: Arc<dyn MessageStore>,
        auth: AuthConfig,
        openai_chat: OpenAiChatConfig,
        multimodal: MultimodalConfig,
        upload_path: String,
        chunking: ChunkingConfig,
    ) -> Self {
        let timeout_secs = openai_chat.timeout_secs.max(1);
        let multimodal_timeout = multimodal.timeout_secs.max(1);
        Self {
            embedder,
            store: store.clone(),
            auth_store,
            user_memory,
            messages,
            manager_runtime: None,
            acp_ws: None,
            acp_discovery: None,
            presence: Arc::new(PresenceTracker::default()),
            tombstones: Arc::new(TombstoneTracker::default()),
            message_notify: Arc::new(tokio::sync::Notify::new()),
            message_broadcast: Arc::new(tokio::sync::broadcast::channel(512).0),
            auth: Arc::new(auth),
            openai_chat: Arc::new(openai_chat),
            multimodal: Arc::new(multimodal),
            upload_path: Arc::new(upload_path),
            chunking: Arc::new(chunking),
            md_chunker: None,
            reranker: None,
            reranker_top_n: 50,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .expect("http client should build"),
            multimodal_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(multimodal_timeout))
                .build()
                .expect("multimodal http client should build"),
            analysis: Arc::new(AnalysisConfig::default()),
            dreaming: Arc::new(DreamingConfig::default()),
            ontology: Arc::new(OntologyConfig::default()),
            ontology_llm: Arc::new(OpenAiChatConfig::default()),
            schema_cache: Arc::new(crate::validation::SchemaCache::new()),
            pending_tokens: Arc::new(auth::PendingTokenCache::default()),
            oauth_creds: None,
            google_oauth: Arc::new(GoogleOAuthConfig::default()),
            oauth_token_key: None,
            push: None,
            web_push: Arc::new(WebPushConfig::default()),
            whisper: Arc::new(crate::config::WhisperConfig::default()),
            projection_worker: Arc::new(crate::projection::ProjectionWorker::new(store.clone())),
            cms_runtime: Arc::new(crate::cms::CmsRuntime::new(store.clone())),
        }
    }

    /// Wire the Web Push backend + VAPID config. Call once during startup.
    /// `None` for the store disables all push paths (endpoints will 503).
    pub fn with_web_push(mut self, config: WebPushConfig, store: Arc<dyn PushStore>) -> Self {
        self.web_push = Arc::new(config);
        self.push = Some(store);
        self
    }

    /// Wire the per-user OAuth credential store + Google client config +
    /// master encryption key for the integrations endpoints. Call once
    /// during startup; safe to omit when integrations are disabled.
    pub fn with_google_oauth(
        mut self,
        config: GoogleOAuthConfig,
        store: Arc<dyn OAuthCredsStore>,
        key: Option<Arc<EncryptionKey>>,
    ) -> Self {
        self.google_oauth = Arc::new(config);
        self.oauth_creds = Some(store);
        self.oauth_token_key = key;
        self
    }

    pub fn with_analysis(mut self, analysis: AnalysisConfig) -> Self {
        self.analysis = Arc::new(analysis);
        self
    }

    pub fn with_ontology(mut self, ontology: OntologyConfig, llm: OpenAiChatConfig) -> Self {
        self.ontology = Arc::new(ontology);
        self.ontology_llm = Arc::new(llm);
        self
    }

    pub fn with_dreaming(mut self, dreaming: DreamingConfig) -> Self {
        self.dreaming = Arc::new(dreaming);
        self
    }

    pub fn with_manager(mut self, manager: ManagerConfig) -> Self {
        self.manager_runtime = Some(Arc::new(manager));
        self
    }

    pub fn with_whisper(mut self, whisper: crate::config::WhisperConfig) -> Self {
        self.whisper = Arc::new(whisper);
        self
    }

    pub fn with_reranker(
        mut self,
        reranker: Arc<dyn crate::reranker::Reranker>,
        top_n: usize,
    ) -> Self {
        self.reranker = Some(reranker);
        self.reranker_top_n = top_n.max(1);
        self
    }

    /// Publish a freshly-inserted message: wake long-poll listeners on
    /// `message_notify` and broadcast the record on `message_broadcast` for
    /// per-message subscribers. Call exactly once per persisted message.
    pub fn publish_message(&self, record: &MessageRecord) {
        let _ = self.message_broadcast.send(record.clone());
        self.message_notify.notify_waiters();
    }

    pub fn mcp_allowed_hosts(&self) -> Vec<String> {
        self.auth.mcp_allowed_hosts.clone()
    }

    #[cfg(test)]
    pub fn new_ready(
        embedder: Arc<dyn EmbeddingService>,
        store: Arc<dyn VectorStore>,
        auth_store: Arc<dyn AuthStore>,
    ) -> Self {
        let openai_chat = OpenAiChatConfig {
            timeout_secs: 60,
            ..OpenAiChatConfig::default()
        };
        Self {
            embedder: Arc::new(EmbedderHandle::ready(embedder)),
            store: store.clone(),
            auth_store,
            user_memory: Arc::new(NoopUserMemory),
            messages: Arc::new(NoopMessages),
            manager_runtime: None,
            acp_ws: None,
            acp_discovery: None,
            presence: Arc::new(PresenceTracker::default()),
            tombstones: Arc::new(TombstoneTracker::default()),
            message_notify: Arc::new(tokio::sync::Notify::new()),
            message_broadcast: Arc::new(tokio::sync::broadcast::channel(512).0),
            auth: Arc::new(AuthConfig::default()),
            openai_chat: Arc::new(openai_chat),
            multimodal: Arc::new(MultimodalConfig::default()),
            upload_path: Arc::new("uploads".to_owned()),
            chunking: Arc::new(ChunkingConfig::default()),
            md_chunker: None,
            reranker: None,
            reranker_top_n: 50,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("http client should build"),
            multimodal_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("multimodal http client should build"),
            analysis: Arc::new(AnalysisConfig::default()),
            dreaming: Arc::new(DreamingConfig::default()),
            ontology: Arc::new(OntologyConfig::default()),
            ontology_llm: Arc::new(OpenAiChatConfig::default()),
            schema_cache: Arc::new(crate::validation::SchemaCache::new()),
            pending_tokens: Arc::new(auth::PendingTokenCache::default()),
            oauth_creds: None,
            google_oauth: Arc::new(GoogleOAuthConfig::default()),
            oauth_token_key: None,
            push: None,
            web_push: Arc::new(WebPushConfig::default()),
            whisper: Arc::new(crate::config::WhisperConfig::default()),
            projection_worker: Arc::new(crate::projection::ProjectionWorker::new(store.clone())),
            cms_runtime: Arc::new(crate::cms::CmsRuntime::new(store.clone())),
        }
    }
}

pub struct EmbedderHandle {
    inner: RwLock<EmbedderState>,
}

enum EmbedderState {
    Loading,
    Ready(Arc<dyn EmbeddingService>),
    Failed(String),
}

impl EmbedderHandle {
    pub fn loading() -> Self {
        Self {
            inner: RwLock::new(EmbedderState::Loading),
        }
    }

    pub fn ready(embedder: Arc<dyn EmbeddingService>) -> Self {
        Self {
            inner: RwLock::new(EmbedderState::Ready(embedder)),
        }
    }

    pub fn mark_ready(&self, embedder: Arc<dyn EmbeddingService>) {
        *self.inner.write().expect("embedder state lock poisoned") = EmbedderState::Ready(embedder);
    }

    pub fn mark_failed(&self, error: String) {
        *self.inner.write().expect("embedder state lock poisoned") = EmbedderState::Failed(error);
    }

    /// Public, non-API variant of `get_ready` for use by background workers
    /// (e.g. code-ingest). Returns `anyhow::Error` so it composes with
    /// non-axum call sites.
    pub fn try_ready(&self) -> anyhow::Result<Arc<dyn EmbeddingService>> {
        match &*self.inner.read().expect("embedder state lock poisoned") {
            EmbedderState::Loading => anyhow::bail!("embedder is still loading"),
            EmbedderState::Ready(embedder) => Ok(embedder.clone()),
            EmbedderState::Failed(error) => {
                anyhow::bail!("embedder failed to initialize: {error}")
            }
        }
    }

    pub(crate) fn get_ready(&self) -> Result<Arc<dyn EmbeddingService>, ApiError> {
        match &*self.inner.read().expect("embedder state lock poisoned") {
            EmbedderState::Loading => Err(ApiError::ServiceUnavailable(
                "embedder is still loading".to_owned(),
            )),
            EmbedderState::Ready(embedder) => Ok(embedder.clone()),
            EmbedderState::Failed(error) => Err(ApiError::ServiceUnavailable(format!(
                "embedder failed to initialize: {error}"
            ))),
        }
    }

    pub(crate) fn health(&self) -> (StatusCode, Json<HealthResponse>) {
        match &*self.inner.read().expect("embedder state lock poisoned") {
            EmbedderState::Loading => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "loading".to_owned(),
                    error: None,
                }),
            ),
            EmbedderState::Ready(_) => (
                StatusCode::OK,
                Json(HealthResponse {
                    status: "ready".to_owned(),
                    error: None,
                }),
            ),
            EmbedderState::Failed(error) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "failed".to_owned(),
                    error: Some(error.clone()),
                }),
            ),
        }
    }
}

#[allow(dead_code)]
pub(crate) struct NoopMessages;

impl MessageStore for NoopMessages {
    fn get_message(&self, _: &str) -> anyhow::Result<Option<MessageRecord>> {
        Ok(None)
    }
    fn send_message(&self, _: NewMessage) -> anyhow::Result<MessageRecord> {
        anyhow::bail!("messages disabled in this build")
    }
    fn update_message(
        &self,
        _: &str,
        _: MessageUpdate,
        _: i64,
    ) -> anyhow::Result<Option<MessageRecord>> {
        Ok(None)
    }
    fn delete_message(&self, _: &str) -> anyhow::Result<Option<MessageRecord>> {
        Ok(None)
    }
    fn find_permission_request(&self, _: &str) -> anyhow::Result<Vec<MessageRecord>> {
        Ok(Vec::new())
    }
    fn list_channel_messages(&self, _: &str) -> anyhow::Result<Vec<MessageRecord>> {
        Ok(Vec::new())
    }
    fn clear_channel(&self, _: &str) -> anyhow::Result<Vec<MessageRecord>> {
        Ok(Vec::new())
    }
    fn list_messages(&self, _: MessageQuery) -> anyhow::Result<(Vec<MessageRecord>, i64)> {
        Ok((Vec::new(), 0))
    }
    fn list_channels(&self) -> anyhow::Result<Vec<ChannelSummary>> {
        Ok(Vec::new())
    }
}

#[allow(dead_code)]
pub(crate) struct NoopUserMemory;

impl UserMemoryStore for NoopUserMemory {
    fn log_user_event(&self, _: NewUserEvent) -> anyhow::Result<()> {
        Ok(())
    }
    fn touch_item_accesses(&self, _: &[String], _: i64) -> anyhow::Result<()> {
        Ok(())
    }
    fn get_user_profile(&self, _: &str) -> anyhow::Result<Option<crate::db::UserProfile>> {
        Ok(None)
    }
    fn upsert_user_profile(&self, _: crate::db::UserProfile) -> anyhow::Result<()> {
        Ok(())
    }
    fn get_recent_query_embeddings(&self, _: &str, _: usize) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
    fn count_events_since(&self, _: &str, _: i64) -> anyhow::Result<i64> {
        Ok(0)
    }
}

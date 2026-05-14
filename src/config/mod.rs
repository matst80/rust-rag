use crate::{db::GraphConfig, embedding::Pooling};
use anyhow::{Context, Result, anyhow};
use std::{
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyConfig {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub frontend_api_key: Option<String>,
    pub session_secret: Option<String>,
    pub api_keys: Vec<ApiKeyConfig>,
    pub app_base_url: Option<String>,
    pub device_code_ttl_secs: u64,
    pub device_code_interval_secs: u64,
    pub mcp_token_ttl_days: Option<u64>,
    pub mcp_allowed_hosts: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            frontend_api_key: None,
            session_secret: None,
            api_keys: Vec::new(),
            app_base_url: None,
            device_code_ttl_secs: 600,
            device_code_interval_secs: 5,
            mcp_token_ttl_days: None,
            mcp_allowed_hosts: default_mcp_allowed_hosts(),
        }
    }
}

fn default_mcp_allowed_hosts() -> Vec<String> {
    vec!["localhost".into(), "127.0.0.1".into(), "::1".into()]
}

fn default_google_scopes() -> Vec<String> {
    vec![
        "openid".into(),
        "email".into(),
        "profile".into(),
        "https://www.googleapis.com/auth/gmail.readonly".into(),
        "https://www.googleapis.com/auth/calendar".into(),
        "https://www.googleapis.com/auth/drive.readonly".into(),
    ]
}

#[derive(Debug, Clone)]
pub struct OpenAiChatConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
    pub timeout_secs: u64,
    pub cdp_url: Option<String>,
    pub retrieval_system_prompt: String,
    pub query_expansion_prompt: String,
}

impl Default for OpenAiChatConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            api_key: None,
            default_model: None,
            timeout_secs: 60,
            cdp_url: None,
            retrieval_system_prompt: default_retrieval_system_prompt().to_owned(),
            query_expansion_prompt: default_query_expansion_prompt().to_owned(),
        }
    }
}

impl OpenAiChatConfig {
    pub fn is_configured(&self) -> bool {
        self.base_url.is_some()
    }
}

pub const fn default_retrieval_system_prompt() -> &'static str {
    "You are a RAG Intelligence Assistant. Your goal is to build and query a high-quality knowledge base.\n\nCORE GUIDELINES:\n1. CRAWLING: Use 'ingest_web_content' to research new information.\n2. LARGE PAGES: If a page is too large (>20k chars), it will be saved to disk. Use 'read_file_range' to read it line-by-line.\n3. CHUNKING: NEVER store a whole page as a single entry. It ruins embedding quality.\n4. EXTRACTION: When you ingest a page, extract specific, meaningful sections.\n5. STORAGE: Use 'store_entry' to save semantically coherent chunks (typically 2000-4000 characters). NEVER split code blocks — keep them intact with their surrounding explanation.\n6. CONTEXT: Ensure each stored chunk is self-contained (include relevant titles/context in the text).\n7. RETRIEVAL: When a user asks a question, decompose it into multiple focused 'search_entries' calls from different angles (synonyms, sub-topics, related concepts) rather than a single literal query. Prefer hybrid search. Merge and cite the best results.\n\nBe concise and analytical."
}

pub const fn default_query_expansion_prompt() -> &'static str {
    "You expand a user's natural-language information need into a diverse set of focused search queries for a hybrid vector + BM25 search engine over a private knowledge base.\n\nRULES:\n- Return ONLY a JSON array of strings, no prose, no code fences.\n- Produce 3-6 queries.\n- Cover different angles: literal rewording, key entities, synonyms, related sub-topics, and one broader conceptual query.\n- Each query must be self-contained (no pronouns, no references to prior queries).\n- Keep each query concise (under 15 words) and in the language the user wrote in.\n\nExample output: [\"first query\", \"second query\", \"third query\"]"
}

impl AuthConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn matches_api_key(&self, candidate: &str) -> bool {
        let candidate = candidate.trim();
        if candidate.is_empty() {
            return false;
        }

        self.frontend_api_key.as_deref() == Some(candidate)
            || self.api_keys.iter().any(|key| key.value == candidate)
    }
}

#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Maximum characters per stored chunk. Tune to your embedding model's context window.
    /// Env: RAG_CHUNK_MAX_CHARS (default 1536)
    pub chunk_max_chars: usize,
    /// Characters of the previous chunk's tail to prepend to each chunk's embedding for context.
    /// Env: RAG_CHUNK_OVERLAP_CHARS (default 200)
    pub chunk_overlap_chars: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_max_chars: 1536,
            chunk_overlap_chars: 200,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MultimodalConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub timeout_secs: u64,
}

impl Default for MultimodalConfig {
    fn default() -> Self {
        Self { base_url: None, api_key: None, model: None, timeout_secs: 120 }
    }
}

impl MultimodalConfig {
    pub fn is_configured(&self) -> bool {
        self.base_url.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct OntologyConfig {
    pub enabled: bool,
    pub confidence_threshold: f32,
    pub batch_size: usize,
    pub interval_secs: u64,
    /// Number of nearest neighbors to retrieve and send to the LLM per item.
    /// Reduce for local models with small context windows.
    pub neighbor_count: usize,
    /// Max characters of the target item's text to include in the LLM prompt.
    pub target_preview_chars: usize,
    /// Max characters of each candidate item's text to include in the LLM prompt.
    pub candidate_preview_chars: usize,
}

impl Default for OntologyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            confidence_threshold: 0.7,
            batch_size: 5,
            interval_secs: 30,
            neighbor_count: 8,
            target_preview_chars: 600,
            candidate_preview_chars: 300,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagerConfig {
    pub enabled: bool,
    pub channel: String,
    pub mention: String,
    pub interval_secs: u64,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub timeout_secs: Option<u64>,
    pub system_prompt: String,
    pub max_iterations: usize,
    pub memory_source_id: String,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channel: "manager".to_owned(),
            mention: "@manager".to_owned(),
            interval_secs: 300,
            base_url: None,
            api_key: None,
            model: None,
            timeout_secs: None,
            system_prompt: default_manager_system_prompt().to_owned(),
            max_iterations: 8,
            memory_source_id: "manager_memory".to_owned(),
        }
    }
}

pub const fn default_manager_system_prompt() -> &'static str {
    "You are the Manager: an autonomous orchestrator coordinating humans and ACP agents.\n\nYOUR ROLE:\n- Read two input streams: rust-rag channels (human conversation, agent handoffs) and ACP WebSocket events (agent session lifecycle, permission requests).\n- Drive ACP agents directly via the acp_* tools (NOT by posting magic strings to channels — that bridge is gone).\n- Maintain durable memory in `manager_memory`.\n- Use the RAG knowledge base for shared context.\n\nNAMESPACES (source_id discipline):\n- `manager_memory` — orchestrator notes + tasks.\n- `knowledge` — cross-project evergreen facts.\n- `project:<slug>:knowledge` / `project:<slug>:todos` — per-project buckets.\nDetect <slug> from channel name, project_path, or task context.\n\nTRIGGER CONTEXT:\nEach invocation includes a `trigger`: `manager_channel` (user posted in your channel), `mention` (@mention elsewhere), `cron` (interval tick), `acp_event` (PermissionRequest or SessionEnded needs attention).\n\nACP CONTROL (via WebSocket tools):\nCommands (fire-and-forget; observe outcome via events):\n- `acp_spawn { project_path, agent_command?, metadata? }` — start a headless session. New session id arrives as a `SessionStarted` event.\n- `acp_send_prompt { session_id, text }` — send user-facing prompt.\n- `acp_cancel { session_id }` — interrupt running prompt.\n- `acp_end_session { session_id }` — graceful termination.\n- `acp_set_permission_mode { session_id, mode: auto|manual }` — switch tool-approval mode (default `manual` for headless).\n- `acp_set_config { session_id, key, value }`.\n- `acp_list_sessions` — request fresh state; result arrives as event.\n- `acp_permission_respond { request_id, decision }` — answer outstanding PermissionRequest. decision ∈ allow_once|allow_always|deny|deny_always.\n\nReads (in-process WS event ring buffer, ~200 events per session, ephemeral):\n- `acp_recent_events { session_id?, since_local_seq?, kinds?, limit? }` — inspect recent agent activity.\n- `acp_pending_permissions` — outstanding PermissionRequest events awaiting decision.\n- `acp_get_snapshot` — last full Snapshot event (covers history for late-attached sessions).\n\nORCHESTRATION TOOLS (rust-rag side):\n- `list_agents`, `channel_summary` — see who is online and channel load.\n- `assign_task` / `list_tasks` / `update_task` — durable task tracking.\n\nBOOT LOOP (every non-trivial trigger):\n1. `recall` from manager_memory for prior orchestrator state.\n2. `search_rag` scoped to `project:<slug>:knowledge` for project context, then unscoped for cross-project hits.\n3. If trigger=acp_event: call `acp_pending_permissions` and `acp_recent_events` to load WS state.\n4. Read the canonical entry `agent_collaboration_guide` once per session.\n\nROUTING POLICY:\n1. Identify the project slug + domain from the request.\n2. Pick or spawn an ACP session via `acp_spawn` (headless) when work is real (code, research, etc.).\n3. Inject relevant RAG context into the prompt before `acp_send_prompt`.\n4. Track via `assign_task` so progress is durable.\n\nAUTO-RAG-INJECTION:\n- For `mention` and `manager_channel` triggers, `search_rag` first (project namespace, then global).\n- Synthesize 3-5 bullets back into the requesting channel; never dump raw chunks.\n\nPERMISSION HANDLING:\n- On `acp_event` with PermissionRequest, decide quickly: deny by default for destructive tools (rm, drop, force-push); allow_once for read-only / scoped operations.\n- Always answer with `acp_permission_respond` — leaving requests pending blocks the agent.\n\nPERSIST DISCIPLINE:\n- Stable descriptive ids (no UUIDs). Always `metadata.author = \"claude-manager\"` + `metadata.tags`.\n- Update existing entries with `update_item` over creating duplicates.\n- Never store secrets, PII, or content trivially derivable from git/messages.\n\nRESPONSE STYLE:\n- Be terse. Act, then briefly post status to your own channel if user-visible explanation matters.\n- If no action warranted (e.g. cron tick with nothing new), call no tools and produce no output."
}

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub enabled: bool,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub timeout_secs: u64,
    pub max_neighbors: usize,
    pub neighbor_threshold: f32,
    pub cross_source: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: None,
            api_key: None,
            model: None,
            timeout_secs: 30,
            max_neighbors: 8,
            neighbor_threshold: 0.65,
            cross_source: true,
        }
    }
}

impl AnalysisConfig {
    pub fn is_configured(&self) -> bool {
        self.enabled && self.base_url.is_some() && self.model.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct DreamingConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub batch_size: usize,
    pub source_id: String,
    pub target_source_id: String,
}

impl Default for DreamingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 3600,
            batch_size: 5,
            source_id: "memory".to_owned(),
            target_source_id: "knowledge".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub db_path: String,
    /// Optional Postgres URL. When set, the server connects (and runs
    /// migrations) at startup. Active store remains SQLite during cutover.
    pub database_url: Option<String>,
    pub upload_path: String,
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub ort_dylib_path: Option<PathBuf>,
    pub embedding_dimension: usize,
    pub embedding_pooling: Pooling,
    pub intra_threads: usize,
    pub graph_enabled: bool,
    pub graph_build_on_startup: bool,
    pub graph_similarity_top_k: usize,
    pub graph_similarity_max_distance: f32,
    pub graph_cross_source: bool,
    pub auth: AuthConfig,
    pub openai_chat: OpenAiChatConfig,
    pub multimodal: MultimodalConfig,
    pub chunking: ChunkingConfig,
    pub ontology: OntologyConfig,
    pub manager: ManagerConfig,
    pub acp_ws: AcpWsConfig,
    pub analysis: AnalysisConfig,
    pub dreaming: DreamingConfig,
    pub google_oauth: GoogleOAuthConfig,
    pub web_push: WebPushConfig,
}

#[derive(Debug, Clone, Default)]
pub struct GoogleOAuthConfig {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    /// Public redirect URL, e.g. `https://rag.example.com/api/integrations/google/callback`.
    /// Must match what is registered in the GCP OAuth client.
    pub redirect_uri: Option<String>,
    /// Master key for AES-GCM-encrypting stored tokens (32 bytes, base64 or hex).
    pub token_enc_key: Option<String>,
    /// Default OAuth scopes requested on /start. Comma-separated.
    pub default_scopes: Vec<String>,
}

impl GoogleOAuthConfig {
    pub fn is_configured(&self) -> bool {
        self.client_id.is_some()
            && self.client_secret.is_some()
            && self.redirect_uri.is_some()
            && self.token_enc_key.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct WebPushConfig {
    /// VAPID public key (base64-url, uncompressed P-256 point — what the
    /// browser's `pushManager.subscribe()` accepts as `applicationServerKey`).
    pub public_key: Option<String>,
    /// VAPID private key (base64-url, 32-byte P-256 scalar).
    pub private_key: Option<String>,
    /// `mailto:` URI or origin URL embedded in VAPID JWTs — push services
    /// use it to contact the operator if your traffic misbehaves.
    pub subject: Option<String>,
}

impl WebPushConfig {
    pub fn is_configured(&self) -> bool {
        self.public_key.is_some() && self.private_key.is_some() && self.subject.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct AcpWsConfig {
    pub url: Option<String>,
    pub token: Option<String>,
    pub ring_buffer_per_session: usize,
    pub reconnect_initial_secs: u64,
    pub reconnect_max_secs: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let frontend_api_key = non_empty_var("RAG_FRONTEND_API_KEY");
        let session_secret = non_empty_var("AUTH_SESSION_SECRET");
        let api_keys = parse_api_keys(env::var("RAG_API_KEYS").ok())?;
        let openai_api_key = non_empty_var("RAG_OPENAI_API_KEY");
        let openai_base_url = non_empty_var("RAG_OPENAI_API_BASE_URL")
            .or_else(|| {
                openai_api_key
                    .as_ref()
                    .map(|_| "https://api.openai.com/v1".to_owned())
            })
            .map(|value| value.trim_end_matches('/').to_owned());
        let openai_default_model = non_empty_var("RAG_OPENAI_MODEL");
        let openai_timeout_secs = parse_env("RAG_OPENAI_TIMEOUT_SECS", "60")?;
        let cdp_url = non_empty_var("RAG_CDP_URL");

        let auth_enabled = match env::var("RAG_AUTH_ENABLED") {
            Ok(raw) => raw
                .parse::<bool>()
                .map_err(|error| anyhow!("failed to parse RAG_AUTH_ENABLED={raw:?}: {error}"))?,
            Err(env::VarError::NotPresent) => {
                frontend_api_key.is_some() || session_secret.is_some() || !api_keys.is_empty()
            }
            Err(error) => return Err(anyhow!("failed to read RAG_AUTH_ENABLED: {error}")),
        };

        Ok(Self {
            host: parse_env("RAG_HOST", "0.0.0.0")?,
            port: parse_env("RAG_PORT", "4001")?,
            db_path: env::var("RAG_DB_PATH").unwrap_or_else(|_| "rag.db".to_owned()),
            database_url: non_empty_var("RAG_DATABASE_URL"),
            upload_path: env::var("RAG_UPLOAD_PATH").unwrap_or_else(|_| "uploads".to_owned()),
            model_path: required_path("RAG_MODEL_PATH")?,
            tokenizer_path: required_path("RAG_TOKENIZER_PATH")?,
            ort_dylib_path: env::var_os("RAG_ORT_DYLIB_PATH").map(PathBuf::from),
            embedding_dimension: parse_env("RAG_EMBEDDING_DIMENSION", "384")?,
            embedding_pooling: match non_empty_var("RAG_EMBEDDING_POOLING") {
                Some(value) => value
                    .parse()
                    .map_err(|err| anyhow!("RAG_EMBEDDING_POOLING={value:?}: {err}"))?,
                None => Pooling::Mean,
            },
            intra_threads: parse_env("RAG_INTRA_THREADS", "2")?,
            graph_enabled: parse_env("RAG_GRAPH_ENABLED", "false")?,
            graph_build_on_startup: parse_env("RAG_GRAPH_BUILD_ON_STARTUP", "false")?,
            graph_similarity_top_k: parse_env("RAG_GRAPH_K", "5")?,
            graph_similarity_max_distance: parse_env("RAG_GRAPH_MAX_DISTANCE", "0.75")?,
            graph_cross_source: parse_env("RAG_GRAPH_CROSS_SOURCE", "true")?,
            auth: AuthConfig {
                enabled: auth_enabled,
                frontend_api_key,
                session_secret,
                api_keys,
                app_base_url: non_empty_var("RAG_APP_BASE_URL")
                    .or_else(|| non_empty_var("APP_BASE_URL"))
                    .map(|value| value.trim_end_matches('/').to_owned()),
                device_code_ttl_secs: parse_env("RAG_DEVICE_CODE_TTL_SECS", "600")?,
                device_code_interval_secs: parse_env("RAG_DEVICE_CODE_INTERVAL_SECS", "5")?,
                mcp_token_ttl_days: non_empty_var("RAG_MCP_TOKEN_TTL_DAYS")
                    .map(|raw| {
                        raw.parse::<u64>().map_err(|error| {
                            anyhow!("failed to parse RAG_MCP_TOKEN_TTL_DAYS={raw:?}: {error}")
                        })
                    })
                    .transpose()?,
                mcp_allowed_hosts: parse_csv_env("RAG_MCP_ALLOWED_HOSTS")
                    .unwrap_or_else(default_mcp_allowed_hosts),
            },
            openai_chat: OpenAiChatConfig {
                base_url: openai_base_url,
                api_key: openai_api_key,
                default_model: openai_default_model,
                timeout_secs: openai_timeout_secs,
                cdp_url,
                retrieval_system_prompt: non_empty_var("RAG_RETRIEVAL_SYSTEM_PROMPT")
                    .unwrap_or_else(|| default_retrieval_system_prompt().to_owned()),
                query_expansion_prompt: non_empty_var("RAG_QUERY_EXPANSION_PROMPT")
                    .unwrap_or_else(|| default_query_expansion_prompt().to_owned()),
            },
            multimodal: MultimodalConfig {
                base_url: non_empty_var("RAG_MULTIMODAL_BASE_URL")
                    .map(|v| v.trim_end_matches('/').to_owned()),
                api_key: non_empty_var("RAG_MULTIMODAL_API_KEY"),
                model: non_empty_var("RAG_MULTIMODAL_MODEL"),
                timeout_secs: parse_env("RAG_MULTIMODAL_TIMEOUT_SECS", "120")?,
            },
            chunking: {
                let chunk_max_chars: usize = parse_env("RAG_CHUNK_MAX_CHARS", "1536")?;
                let chunk_overlap_chars: usize = parse_env("RAG_CHUNK_OVERLAP_CHARS", "200")?;
                ChunkingConfig { chunk_max_chars, chunk_overlap_chars }
            },
            manager: ManagerConfig {
                enabled: parse_env("RAG_MANAGER_ENABLED", "false")?,
                channel: env::var("RAG_MANAGER_CHANNEL")
                    .unwrap_or_else(|_| "manager".to_owned()),
                mention: env::var("RAG_MANAGER_MENTION")
                    .unwrap_or_else(|_| "@manager".to_owned()),
                interval_secs: parse_env("RAG_MANAGER_INTERVAL_SECS", "300")?,
                base_url: non_empty_var("RAG_MANAGER_API_BASE_URL")
                    .map(|v| v.trim_end_matches('/').to_owned()),
                api_key: non_empty_var("RAG_MANAGER_API_KEY"),
                model: non_empty_var("RAG_MANAGER_MODEL"),
                timeout_secs: non_empty_var("RAG_MANAGER_TIMEOUT_SECS")
                    .map(|v| v.parse::<u64>())
                    .transpose()
                    .map_err(|e| anyhow!("failed to parse RAG_MANAGER_TIMEOUT_SECS: {e}"))?,
                system_prompt: non_empty_var("RAG_MANAGER_SYSTEM_PROMPT")
                    .unwrap_or_else(|| default_manager_system_prompt().to_owned()),
                max_iterations: parse_env("RAG_MANAGER_MAX_ITERATIONS", "8")?,
                memory_source_id: env::var("RAG_MANAGER_MEMORY_SOURCE_ID")
                    .unwrap_or_else(|_| "manager_memory".to_owned()),
            },
            acp_ws: AcpWsConfig {
                url: non_empty_var("RAG_ACP_WS_URL"),
                token: non_empty_var("RAG_ACP_WS_TOKEN")
                    .or_else(|| non_empty_var("ACP_WS_TOKEN"))
                    .or_else(|| non_empty_var("TELEGRAM_ACP_WS_TOKEN")),
                ring_buffer_per_session: parse_env("RAG_ACP_WS_BUFFER", "200")?,
                reconnect_initial_secs: parse_env("RAG_ACP_WS_RECONNECT_INITIAL_SECS", "1")?,
                reconnect_max_secs: parse_env("RAG_ACP_WS_RECONNECT_MAX_SECS", "30")?,
            },
            analysis: AnalysisConfig {
                enabled: parse_env("RAG_ANALYSIS_ENABLED", "false")?,
                base_url: non_empty_var("RAG_ANALYSIS_BASE_URL")
                    .or_else(|| non_empty_var("RAG_OPENAI_API_BASE_URL"))
                    .map(|v| v.trim_end_matches('/').to_owned()),
                api_key: non_empty_var("RAG_ANALYSIS_API_KEY")
                    .or_else(|| non_empty_var("RAG_OPENAI_API_KEY")),
                model: non_empty_var("RAG_ANALYSIS_MODEL")
                    .or_else(|| non_empty_var("RAG_OPENAI_MODEL")),
                timeout_secs: parse_env("RAG_ANALYSIS_TIMEOUT_SECS", "30")?,
                max_neighbors: parse_env("RAG_ANALYSIS_MAX_NEIGHBORS", "8")?,
                neighbor_threshold: parse_env("RAG_ANALYSIS_NEIGHBOR_THRESHOLD", "0.65")?,
                cross_source: parse_env("RAG_ANALYSIS_CROSS_SOURCE", "true")?,
            },
            ontology: OntologyConfig {
                enabled: parse_env("RAG_ONTOLOGY_ENABLED", "false")?,
                confidence_threshold: parse_env("RAG_ONTOLOGY_CONFIDENCE_THRESHOLD", "0.7")?,
                batch_size: parse_env("RAG_ONTOLOGY_BATCH_SIZE", "5")?,
                interval_secs: parse_env("RAG_ONTOLOGY_INTERVAL_SECS", "30")?,
                neighbor_count: parse_env("RAG_ONTOLOGY_NEIGHBOR_COUNT", "8")?,
                target_preview_chars: parse_env("RAG_ONTOLOGY_TARGET_PREVIEW_CHARS", "600")?,
                candidate_preview_chars: parse_env("RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS", "300")?,
            },
            dreaming: DreamingConfig {
                enabled: parse_env("RAG_DREAMING_ENABLED", "false")?,
                interval_secs: parse_env("RAG_DREAMING_INTERVAL_SECS", "3600")?,
                batch_size: parse_env("RAG_DREAMING_BATCH_SIZE", "5")?,
                source_id: env::var("RAG_DREAMING_SOURCE_ID")
                    .unwrap_or_else(|_| "memory".to_owned()),
                target_source_id: env::var("RAG_DREAMING_TARGET_SOURCE_ID")
                    .unwrap_or_else(|_| "knowledge".to_owned()),
            },
            google_oauth: GoogleOAuthConfig {
                client_id: non_empty_var("GOOGLE_OAUTH_CLIENT_ID"),
                client_secret: non_empty_var("GOOGLE_OAUTH_CLIENT_SECRET"),
                redirect_uri: non_empty_var("GOOGLE_OAUTH_REDIRECT_URI"),
                token_enc_key: non_empty_var("OAUTH_TOKEN_ENC_KEY"),
                default_scopes: parse_csv_env("GOOGLE_OAUTH_DEFAULT_SCOPES")
                    .unwrap_or_else(default_google_scopes),
            },
            web_push: WebPushConfig {
                public_key: non_empty_var("VAPID_PUBLIC_KEY"),
                private_key: non_empty_var("VAPID_PRIVATE_KEY"),
                subject: non_empty_var("VAPID_SUBJECT"),
            },
        })
    }

    pub fn bind_address(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn graph_config(&self) -> GraphConfig {
        GraphConfig {
            enabled: self.graph_enabled,
            build_on_startup: self.graph_build_on_startup,
            similarity_top_k: self.graph_similarity_top_k,
            similarity_max_distance: self.graph_similarity_max_distance,
            cross_source: self.graph_cross_source,
        }
    }
}

fn parse_env<T>(name: &str, default: &str) -> Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let raw = env::var(name).unwrap_or_else(|_| default.to_owned());
    raw.parse::<T>()
        .map_err(|error| anyhow!("failed to parse {name}={raw:?}: {error}"))
}

fn required_path(name: &str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .with_context(|| format!("missing required environment variable {name}"))
}

fn parse_api_keys(raw: Option<String>) -> Result<Vec<ApiKeyConfig>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    let mut api_keys = Vec::new();
    for (index, entry) in raw.split(',').enumerate() {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (name, value) = match trimmed.split_once(':') {
            Some((name, value)) if !name.trim().is_empty() && !value.trim().is_empty() => {
                (name.trim().to_owned(), value.trim().to_owned())
            }
            _ => (format!("key-{}", index + 1), trimmed.to_owned()),
        };

        api_keys.push(ApiKeyConfig { name, value });
    }

    Ok(api_keys)
}

fn non_empty_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_csv_env(name: &str) -> Option<Vec<String>> {
    let raw = non_empty_var(name)?;
    let parts: Vec<String> = raw
        .split(',')
        .map(|part| part.trim().to_owned())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() { None } else { Some(parts) }
}

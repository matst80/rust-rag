use crate::db::GraphConfig;
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
    "You are a RAG Intelligence Assistant. Your goal is to build and query a high-quality knowledge base.\n\nCORE GUIDELINES:\n1. CRAWLING: Use 'ingest_web_content' to research new information.\n2. LARGE PAGES: If a page is too large (>20k chars), it will be saved to disk. Use 'read_file_range' to read it line-by-line.\n3. CHUNKING: NEVER store a whole page as a single entry. It ruins embedding quality.\n4. EXTRACTION: When you ingest a page, extract specific, meaningful sections.\n5. STORAGE: Use 'store_entry' to save focused chunks of 1000-1500 characters.\n6. CONTEXT: Ensure each stored chunk is self-contained (include relevant titles/context in the text).\n7. RETRIEVAL: When a user asks a question, decompose it into multiple focused 'search_entries' calls from different angles (synonyms, sub-topics, related concepts) rather than a single literal query. Prefer hybrid search. Merge and cite the best results.\n\nBe concise and analytical."
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
    /// Items longer than this are flagged as oversized in the admin panel.
    /// Env: RAG_LARGE_ITEM_THRESHOLD (default = chunk_max_chars)
    pub large_item_threshold: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_max_chars: 1536,
            chunk_overlap_chars: 200,
            large_item_threshold: 1536,
        }
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
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub db_path: String,
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub ort_dylib_path: Option<PathBuf>,
    pub embedding_dimension: usize,
    pub intra_threads: usize,
    pub graph_enabled: bool,
    pub graph_build_on_startup: bool,
    pub graph_similarity_top_k: usize,
    pub graph_similarity_max_distance: f32,
    pub graph_cross_source: bool,
    pub auth: AuthConfig,
    pub openai_chat: OpenAiChatConfig,
    pub chunking: ChunkingConfig,
    pub ontology: OntologyConfig,
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
            model_path: required_path("RAG_MODEL_PATH")?,
            tokenizer_path: required_path("RAG_TOKENIZER_PATH")?,
            ort_dylib_path: env::var_os("RAG_ORT_DYLIB_PATH").map(PathBuf::from),
            embedding_dimension: parse_env("RAG_EMBEDDING_DIMENSION", "384")?,
            intra_threads: parse_env("RAG_INTRA_THREADS", "2")?,
            graph_enabled: parse_env("RAG_GRAPH_ENABLED", "false")?,
            graph_build_on_startup: parse_env("RAG_GRAPH_BUILD_ON_STARTUP", "false")?,
            graph_similarity_top_k: parse_env("RAG_GRAPH_K", "5")?,
            graph_similarity_max_distance: parse_env("RAG_GRAPH_MAX_DISTANCE", "0.75")?,
            graph_cross_source: parse_env("RAG_GRAPH_CROSS_SOURCE", "false")?,
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
            chunking: {
                let chunk_max_chars: usize = parse_env("RAG_CHUNK_MAX_CHARS", "1536")?;
                let chunk_overlap_chars: usize = parse_env("RAG_CHUNK_OVERLAP_CHARS", "200")?;
                let large_item_threshold: usize = parse_env(
                    "RAG_LARGE_ITEM_THRESHOLD",
                    &chunk_max_chars.to_string(),
                )?;
                ChunkingConfig { chunk_max_chars, chunk_overlap_chars, large_item_threshold }
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

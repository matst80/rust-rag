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

#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub enabled: bool,
    pub frontend_api_key: Option<String>,
    pub session_secret: Option<String>,
    pub api_keys: Vec<ApiKeyConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenAiChatConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
    pub timeout_secs: u64,
    pub cdp_url: Option<String>,
}

impl OpenAiChatConfig {
    pub fn is_configured(&self) -> bool {
        self.base_url.is_some()
    }
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
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let frontend_api_key = non_empty_var("RAG_FRONTEND_API_KEY");
        let session_secret = non_empty_var("AUTH_SESSION_SECRET");
        let api_keys = parse_api_keys(env::var("RAG_API_KEYS").ok())?;
        let openai_api_key = non_empty_var("RAG_OPENAI_API_KEY");
        let openai_base_url = non_empty_var("RAG_OPENAI_API_BASE_URL")
            .or_else(|| openai_api_key.as_ref().map(|_| "https://api.openai.com/v1".to_owned()))
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
            },
            openai_chat: OpenAiChatConfig {
                base_url: openai_base_url,
                api_key: openai_api_key,
                default_model: openai_default_model,
                timeout_secs: openai_timeout_secs,
                cdp_url,
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

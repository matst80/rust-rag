use crate::db::GraphConfig;
use anyhow::{Context, Result, anyhow};
use std::{
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

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
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
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

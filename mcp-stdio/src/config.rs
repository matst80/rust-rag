use anyhow::{Context, Result, anyhow, bail};
use std::{collections::BTreeSet, env, time::Duration};

const DEFAULT_API_BASE_URL: &str = "https://rag.k6n.net";
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_SERVER_NAME: &str = "rust-rag-mcp";
const DEFAULT_SERVER_INSTRUCTIONS: &str = "This server exposes the rust-rag retrieval store over MCP tools. \
Entries are grouped by a user-defined `source_id` (a short lowercase namespace such as \"memory\", \"knowledge\", or \"notes\") — pick a stable source_id per logical bucket of content. \
Use `search_entries` for semantic retrieval (filter by source_id when you know the bucket), `store_entry` to add content, admin tools to manage items, and graph tools to inspect or curate manual links between entries.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolGroup {
    Core,
    Admin,
    Graph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFormat {
    Markdown,
    Json,
    Both,
}

impl SearchFormat {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "markdown" | "md" | "text" => Ok(Self::Markdown),
            "json" | "structured" => Ok(Self::Json),
            "both" | "all" => Ok(Self::Both),
            other => bail!("unsupported search format {other}"),
        }
    }
}

impl Default for SearchFormat {
    fn default() -> Self {
        Self::Markdown
    }
}

impl ToolGroup {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Admin => "admin",
            Self::Graph => "graph",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "core" => Ok(Self::Core),
            "admin" => Ok(Self::Admin),
            "graph" => Ok(Self::Graph),
            other => bail!("unsupported tool group {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderConfig {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeConfig {
    pub api_base_url: String,
    pub request_timeout: Duration,
    pub enabled_groups: BTreeSet<ToolGroup>,
    pub auth_bearer: Option<String>,
    pub headers: Vec<HeaderConfig>,
    pub server_name: String,
    pub server_version: String,
    pub server_instructions: Option<String>,
    pub search_format: SearchFormat,
}

impl BridgeConfig {
    pub fn from_env() -> Result<Self> {
        Self::from_env_map(env::vars())
    }

    fn from_env_map<I, K, V>(vars: I) -> Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let values = vars
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect::<std::collections::HashMap<String, String>>();

        let api_base_url = values
            .get("RAG_MCP_API_BASE_URL")
            .cloned()
            .unwrap_or_else(|| DEFAULT_API_BASE_URL.to_owned());

        let request_timeout = Duration::from_secs(parse_u64_env(
            values.get("RAG_MCP_TIMEOUT_SECS"),
            DEFAULT_TIMEOUT_SECS,
            "RAG_MCP_TIMEOUT_SECS",
        )?);

        let enabled_groups = parse_tool_groups(values.get("RAG_MCP_TOOL_GROUPS"))?;
        let auth_bearer = non_empty(values.get("RAG_MCP_AUTH_BEARER"));
        let headers = parse_headers(values.get("RAG_MCP_HEADERS"))?;

        let server_name = values
            .get("RAG_MCP_SERVER_NAME")
            .cloned()
            .unwrap_or_else(|| DEFAULT_SERVER_NAME.to_owned());
        let server_version = values
            .get("RAG_MCP_SERVER_VERSION")
            .cloned()
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
        let server_instructions = values
            .get("RAG_MCP_SERVER_INSTRUCTIONS")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(|| Some(DEFAULT_SERVER_INSTRUCTIONS.to_owned()));

        let search_format = match values.get("RAG_MCP_SEARCH_FORMAT") {
            Some(raw) if !raw.trim().is_empty() => SearchFormat::parse(raw)?,
            _ => SearchFormat::default(),
        };

        Ok(Self {
            api_base_url,
            request_timeout,
            enabled_groups,
            auth_bearer,
            headers,
            server_name,
            server_version,
            server_instructions,
            search_format,
        })
    }
}

fn parse_u64_env(value: Option<&String>, default: u64, key: &str) -> Result<u64> {
    match value {
        Some(raw) => raw
            .trim()
            .parse::<u64>()
            .with_context(|| format!("failed to parse {key} as u64")),
        None => Ok(default),
    }
}

fn parse_tool_groups(value: Option<&String>) -> Result<BTreeSet<ToolGroup>> {
    let raw = value.map(String::as_str).unwrap_or("core,admin,graph");
    let mut groups = BTreeSet::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        groups.insert(ToolGroup::parse(trimmed)?);
    }

    if groups.is_empty() {
        return Err(anyhow!("at least one MCP tool group must be enabled"));
    }

    Ok(groups)
}

fn parse_headers(value: Option<&String>) -> Result<Vec<HeaderConfig>> {
    let Some(raw) = value else {
        return Ok(Vec::new());
    };

    raw.split(';')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let (name, value) = entry
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid header entry {entry}"))?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                bail!("invalid header entry {entry}");
            }
            Ok(HeaderConfig {
                name: name.to_owned(),
                value: value.to_owned(),
            })
        })
        .collect()
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{BridgeConfig, SearchFormat, ToolGroup};

    fn vars(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn parses_defaults() {
        let config = BridgeConfig::from_env_map(Vec::<(String, String)>::new()).unwrap();

        assert_eq!(config.api_base_url, "https://rag.k6n.net");
        assert_eq!(config.request_timeout.as_secs(), 30);
        assert_eq!(config.enabled_groups.len(), 3);
        assert!(config.enabled_groups.contains(&ToolGroup::Core));
        assert!(config.enabled_groups.contains(&ToolGroup::Admin));
        assert!(config.enabled_groups.contains(&ToolGroup::Graph));
        assert_eq!(config.server_name, "rust-rag-mcp");
        assert!(config.server_instructions.is_some());
        assert_eq!(config.search_format, SearchFormat::Markdown);
    }

    #[test]
    fn parses_search_format_override() {
        let config =
            BridgeConfig::from_env_map(vars(&[("RAG_MCP_SEARCH_FORMAT", "both")])).unwrap();
        assert_eq!(config.search_format, SearchFormat::Both);

        let config =
            BridgeConfig::from_env_map(vars(&[("RAG_MCP_SEARCH_FORMAT", "json")])).unwrap();
        assert_eq!(config.search_format, SearchFormat::Json);

        let error =
            BridgeConfig::from_env_map(vars(&[("RAG_MCP_SEARCH_FORMAT", "xml")])).unwrap_err();
        assert!(error.to_string().contains("unsupported search format"));
    }

    #[test]
    fn parses_tool_groups_and_headers() {
        let config = BridgeConfig::from_env_map(vars(&[
            ("RAG_MCP_TOOL_GROUPS", "core,graph"),
            ("RAG_MCP_HEADERS", "x-api-key=test; x-tenant = demo "),
            ("RAG_MCP_AUTH_BEARER", "secret"),
        ]))
        .unwrap();

        assert_eq!(config.enabled_groups.len(), 2);
        assert!(config.enabled_groups.contains(&ToolGroup::Core));
        assert!(config.enabled_groups.contains(&ToolGroup::Graph));
        assert_eq!(config.headers.len(), 2);
        assert_eq!(config.headers[0].name, "x-api-key");
        assert_eq!(config.headers[0].value, "test");
        assert_eq!(config.auth_bearer.as_deref(), Some("secret"));
    }

    #[test]
    fn rejects_empty_tool_group_set() {
        let error =
            BridgeConfig::from_env_map(vars(&[("RAG_MCP_TOOL_GROUPS", " , ")])).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("at least one MCP tool group must be enabled")
        );
    }
}

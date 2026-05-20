//! File-extension → language detection.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    Ts,
    Tsx,
    Js,
    Jsx,
    Python,
    Go,
    Markdown,
    Json,
    Yaml,
    Toml,
    Sql,
    Sh,
    Other,
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Ts => "typescript",
            Lang::Tsx => "tsx",
            Lang::Js => "javascript",
            Lang::Jsx => "jsx",
            Lang::Python => "python",
            Lang::Go => "go",
            Lang::Markdown => "markdown",
            Lang::Json => "json",
            Lang::Yaml => "yaml",
            Lang::Toml => "toml",
            Lang::Sql => "sql",
            Lang::Sh => "sh",
            Lang::Other => "other",
        }
    }

    /// True when the lang has a tree-sitter grammar wired up in
    /// `parser::ParserRegistry`. False → fallback line-window chunking.
    pub fn has_ast_support(self) -> bool {
        matches!(
            self,
            Lang::Rust | Lang::Ts | Lang::Tsx | Lang::Python | Lang::Go
        )
    }
}

pub fn detect_lang(path: &Path) -> Lang {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "rs" => Lang::Rust,
        "ts" => Lang::Ts,
        "tsx" => Lang::Tsx,
        "js" | "mjs" | "cjs" => Lang::Js,
        "jsx" => Lang::Jsx,
        "py" | "pyi" => Lang::Python,
        "go" => Lang::Go,
        "md" | "markdown" => Lang::Markdown,
        "json" | "jsonc" => Lang::Json,
        "yml" | "yaml" => Lang::Yaml,
        "toml" => Lang::Toml,
        "sql" => Lang::Sql,
        "sh" | "bash" | "zsh" => Lang::Sh,
        _ => Lang::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_common_langs() {
        assert_eq!(detect_lang(&PathBuf::from("src/main.rs")), Lang::Rust);
        assert_eq!(detect_lang(&PathBuf::from("a/b.TS")), Lang::Ts);
        assert_eq!(detect_lang(&PathBuf::from("app.tsx")), Lang::Tsx);
        assert_eq!(detect_lang(&PathBuf::from("script.py")), Lang::Python);
        assert_eq!(detect_lang(&PathBuf::from("README.md")), Lang::Markdown);
        assert_eq!(detect_lang(&PathBuf::from("noext")), Lang::Other);
    }

    #[test]
    fn ast_support_matches_registry() {
        assert!(Lang::Rust.has_ast_support());
        assert!(!Lang::Markdown.has_ast_support());
    }
}

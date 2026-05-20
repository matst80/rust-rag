//! tree-sitter parser registry. Maps each supported `Lang` to a fresh
//! `tree_sitter::Parser` plus an S-expression query that pulls out symbol
//! nodes (functions, methods, structs, classes, etc.).
//!
//! Parsers are cheap to create per-call but the `Language` handle is shared.
//! Queries are compiled lazily and cached behind `OnceLock`.

use crate::code::lang::Lang;
use anyhow::{Context, Result};
use std::sync::OnceLock;
use tree_sitter::{Language, Parser, Query};

pub struct LangBindings {
    pub language: Language,
    /// S-expr query matching top-level / class-level definitions. Captures:
    ///   - `@def`      whole node (used for chunk extents)
    ///   - `@name`     symbol identifier
    ///   - `@kind`     symbol kind name (literal text from the pattern)
    pub symbol_query: &'static Query,
}

pub fn bindings_for(lang: Lang) -> Option<&'static LangBindings> {
    match lang {
        Lang::Rust => Some(rust_bindings()),
        Lang::Ts => Some(ts_bindings()),
        Lang::Tsx => Some(tsx_bindings()),
        Lang::Python => Some(py_bindings()),
        Lang::Go => Some(go_bindings()),
        _ => None,
    }
}

pub fn new_parser(lang: Lang) -> Result<Option<Parser>> {
    let Some(b) = bindings_for(lang) else {
        return Ok(None);
    };
    let mut parser = Parser::new();
    parser
        .set_language(&b.language)
        .with_context(|| format!("setting tree-sitter language for {}", lang.name()))?;
    Ok(Some(parser))
}

// ----- Rust -----------------------------------------------------------------

const RUST_QUERY: &str = r#"
(function_item name: (identifier) @name) @def
(impl_item) @def
(struct_item name: (type_identifier) @name) @def
(enum_item name: (type_identifier) @name) @def
(trait_item name: (type_identifier) @name) @def
(mod_item name: (identifier) @name) @def
(const_item name: (identifier) @name) @def
(static_item name: (identifier) @name) @def
(type_item name: (type_identifier) @name) @def
"#;

fn rust_bindings() -> &'static LangBindings {
    static CELL: OnceLock<LangBindings> = OnceLock::new();
    CELL.get_or_init(|| {
        let language = tree_sitter_rust::language();
        let query = Query::new(&language, RUST_QUERY).expect("rust symbol query compiles");
        LangBindings {
            language,
            symbol_query: Box::leak(Box::new(query)),
        }
    })
}

// ----- TypeScript / TSX -----------------------------------------------------

const TS_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def
(method_definition name: (property_identifier) @name) @def
(class_declaration name: (type_identifier) @name) @def
(interface_declaration name: (type_identifier) @name) @def
(type_alias_declaration name: (type_identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(lexical_declaration (variable_declarator name: (identifier) @name value: (arrow_function))) @def
(lexical_declaration (variable_declarator name: (identifier) @name value: (function_expression))) @def
"#;

fn ts_bindings() -> &'static LangBindings {
    static CELL: OnceLock<LangBindings> = OnceLock::new();
    CELL.get_or_init(|| {
        let language = tree_sitter_typescript::language_typescript();
        let query = Query::new(&language, TS_QUERY).expect("ts symbol query compiles");
        LangBindings {
            language,
            symbol_query: Box::leak(Box::new(query)),
        }
    })
}

fn tsx_bindings() -> &'static LangBindings {
    static CELL: OnceLock<LangBindings> = OnceLock::new();
    CELL.get_or_init(|| {
        let language = tree_sitter_typescript::language_tsx();
        let query = Query::new(&language, TS_QUERY).expect("tsx symbol query compiles");
        LangBindings {
            language,
            symbol_query: Box::leak(Box::new(query)),
        }
    })
}

// ----- Python ---------------------------------------------------------------

const PY_QUERY: &str = r#"
(function_definition name: (identifier) @name) @def
(class_definition name: (identifier) @name) @def
"#;

fn py_bindings() -> &'static LangBindings {
    static CELL: OnceLock<LangBindings> = OnceLock::new();
    CELL.get_or_init(|| {
        let language = tree_sitter_python::language();
        let query = Query::new(&language, PY_QUERY).expect("python symbol query compiles");
        LangBindings {
            language,
            symbol_query: Box::leak(Box::new(query)),
        }
    })
}

// ----- Go -------------------------------------------------------------------

const GO_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def
(method_declaration name: (field_identifier) @name) @def
(type_declaration (type_spec name: (type_identifier) @name)) @def
"#;

fn go_bindings() -> &'static LangBindings {
    static CELL: OnceLock<LangBindings> = OnceLock::new();
    CELL.get_or_init(|| {
        let language = tree_sitter_go::language();
        let query = Query::new(&language, GO_QUERY).expect("go symbol query compiles");
        LangBindings {
            language,
            symbol_query: Box::leak(Box::new(query)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_supported_langs_have_bindings() {
        for lang in [Lang::Rust, Lang::Ts, Lang::Tsx, Lang::Python, Lang::Go] {
            assert!(bindings_for(lang).is_some(), "missing bindings: {:?}", lang);
            assert!(new_parser(lang).unwrap().is_some());
        }
    }

    #[test]
    fn unsupported_lang_returns_none() {
        assert!(bindings_for(Lang::Markdown).is_none());
        assert!(new_parser(Lang::Markdown).unwrap().is_none());
    }
}

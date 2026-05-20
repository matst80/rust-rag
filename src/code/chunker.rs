//! File analysis: produces `CodeChunk`s for embedding plus file-level
//! metadata (summary, role, imports, outline, todos) used by `code_*` MCP
//! tools and the watcher's upsert path.
//!
//! Strategy
//! --------
//! 1. Detect `Lang` via `lang::detect_lang`.
//! 2. If a tree-sitter binding exists (`parser::bindings_for`), walk the
//!    `symbol_query` matches → one chunk per top-level / class-level symbol.
//!    Large bodies (> `max_bytes`) get split on blank lines into sub-chunks
//!    sharing the parent's symbol metadata.
//! 3. Otherwise fall back to fixed line-window chunking with overlap.
//! 4. File-level scan extracts the top-of-file doc-comment summary, naive
//!    `imports`, TODO/FIXME markers, and a per-symbol outline.
//!
//! All offsets are recorded against the input string in bytes; line numbers
//! are 1-indexed and computed from byte position.

use crate::code::lang::Lang;
use crate::code::parser::bindings_for;
use crate::db::code::{OutlineEntry, TodoEntry};
use serde::{Deserialize, Serialize};

/// Chunk extracted from a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub kind: String,
    pub name: Option<String>,
    pub symbol_path: Option<String>,
    pub parent_symbol: Option<String>,
    pub visibility: Option<String>,
    pub doc_comment: Option<String>,
    pub signature: Option<String>,
    pub is_test: bool,
    pub is_public: bool,
    pub calls: Vec<String>,
    pub start_line: u32,
    pub end_line: u32,
    pub byte_start: usize,
    pub byte_end: usize,
    pub content: String,
}

/// Result of analyzing a single source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    pub chunks: Vec<CodeChunk>,
    pub summary: Option<String>,
    pub role: Option<String>,
    pub imports: Vec<String>,
    pub outline: Vec<OutlineEntry>,
    pub todos: Vec<TodoEntry>,
    pub line_count: u32,
}

/// Heuristic file-role classification driven by path conventions. Cheap;
/// runs before any parsing.
pub fn detect_role(rel_path: &str) -> Option<&'static str> {
    let p = rel_path.to_ascii_lowercase();
    if p.contains("/tests/") || p.starts_with("tests/") || p.ends_with("_test.go")
        || p.contains(".test.") || p.contains(".spec.") || p.starts_with("test_")
        || p.contains("/test_")
    {
        return Some("test");
    }
    if p.contains("/examples/") || p.starts_with("examples/") {
        return Some("example");
    }
    if p.contains("/benches/") || p.starts_with("benches/") {
        return Some("bench");
    }
    if p.starts_with("src/bin/") || p.ends_with("/main.rs") || p == "main.rs" {
        return Some("bin");
    }
    if p.ends_with("build.rs")
        || p.ends_with("makefile")
        || p.ends_with("dockerfile")
        || p.ends_with(".dockerfile")
        || p.ends_with("cargo.toml")
        || p.ends_with("package.json")
        || p.ends_with("go.mod")
    {
        return Some("build");
    }
    if p.ends_with(".md") || p.ends_with(".markdown") {
        return Some("doc");
    }
    if p.ends_with(".yml")
        || p.ends_with(".yaml")
        || p.ends_with(".toml")
        || p.ends_with(".json")
        || p.ends_with(".jsonc")
    {
        return Some("config");
    }
    if p.ends_with(".sh") || p.ends_with(".bash") {
        return Some("script");
    }
    Some("lib")
}

/// Analyze a source-file's text. `rel_path` is used only for role detection.
pub fn analyze_file(
    rel_path: &str,
    lang: Lang,
    content: &str,
    max_bytes: usize,
) -> FileAnalysis {
    let line_count = content.lines().count() as u32;
    let summary = extract_summary(lang, content);
    let imports = extract_imports(lang, content);
    let todos = extract_todos(content);
    let role = detect_role(rel_path).map(|s| s.to_string());

    let chunks = if lang.has_ast_support() {
        chunk_ast(lang, content, max_bytes)
            .unwrap_or_else(|| chunk_fallback(content, max_bytes))
    } else {
        chunk_fallback(content, max_bytes)
    };

    let outline = chunks
        .iter()
        .filter(|c| c.name.is_some())
        .map(|c| OutlineEntry {
            kind: c.kind.clone(),
            name: c.name.clone().unwrap_or_default(),
            line: c.start_line,
            signature: c.signature.clone(),
            is_public: c.is_public,
            is_test: c.is_test,
        })
        .collect();

    FileAnalysis {
        chunks,
        summary,
        role,
        imports,
        outline,
        todos,
        line_count,
    }
}

// ----- AST chunking ---------------------------------------------------------

fn chunk_ast(lang: Lang, content: &str, max_bytes: usize) -> Option<Vec<CodeChunk>> {
    use tree_sitter::QueryCursor;
    let bindings = bindings_for(lang)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&bindings.language).ok()?;
    let tree = parser.parse(content, None)?;

    let mut cursor = QueryCursor::new();
    let bytes = content.as_bytes();

    // Collect all symbol matches first; sort by byte range; deduplicate
    // overlapping (impl wraps methods — keep impl as parent, drop child
    // methods only when impl alone fits a chunk).
    let mut matches: Vec<(usize, usize, Option<String>, &str)> = Vec::new();
    let mut iter = cursor.matches(bindings.symbol_query, tree.root_node(), bytes);
    while let Some(m) = iter.next() {
        let mut def_node: Option<tree_sitter::Node> = None;
        let mut name: Option<String> = None;
        let mut kind: Option<&str> = None;
        for cap in m.captures {
            let cap_name = &bindings.symbol_query.capture_names()[cap.index as usize];
            match cap_name.as_ref() {
                "def" => {
                    def_node = Some(cap.node);
                    kind = Some(cap.node.kind());
                }
                "name" => {
                    name = cap
                        .node
                        .utf8_text(bytes)
                        .ok()
                        .map(|s| s.to_string());
                }
                _ => {}
            }
        }
        if let (Some(node), Some(kind_str)) = (def_node, kind) {
            matches.push((node.start_byte(), node.end_byte(), name, kind_str));
        }
    }
    if matches.is_empty() {
        return None;
    }
    matches.sort_by_key(|m| (m.0, std::cmp::Reverse(m.1)));

    // Build chunks; reject child symbols whose extent is fully contained in
    // an already-emitted parent that itself fits the size budget. Keeps the
    // outline reasonably granular without exploding chunk count.
    let mut chunks: Vec<CodeChunk> = Vec::new();
    let mut emitted_ranges: Vec<(usize, usize)> = Vec::new();
    for (start, end, name, kind_str) in matches {
        let inside_parent = emitted_ranges
            .iter()
            .any(|(ps, pe)| *ps <= start && end <= *pe);
        if inside_parent {
            continue;
        }
        // Extend `start` backwards to swallow Rust attributes / Python
        // decorators that precede the symbol — they belong with the chunk.
        let start = extend_start_for_decorations(lang, content, start);
        let slice = &content[start..end];
        if slice.len() <= max_bytes {
            let (sl, el) = line_range_for(content, start, end);
            let visibility = detect_visibility(lang, slice);
            let signature = first_meaningful_line(slice);
            let is_public = visibility
                .as_deref()
                .map(|v| v.starts_with("pub") || v == "exported")
                .unwrap_or(false);
            let is_test = looks_like_test(lang, slice, name.as_deref());
            let doc_comment = extract_doc_comment(lang, content, start);
            let calls = extract_calls(slice);
            chunks.push(CodeChunk {
                kind: kind_str.to_string(),
                name: name.clone(),
                symbol_path: name.clone(),
                parent_symbol: None,
                visibility,
                doc_comment,
                signature,
                is_test,
                is_public,
                calls,
                start_line: sl,
                end_line: el,
                byte_start: start,
                byte_end: end,
                content: slice.to_string(),
            });
            emitted_ranges.push((start, end));
        } else {
            // Split oversized body on blank lines.
            for sub in split_on_blank_lines(content, start, end, max_bytes) {
                let (sl, el) = line_range_for(content, sub.0, sub.1);
                let sub_slice = &content[sub.0..sub.1];
                chunks.push(CodeChunk {
                    kind: kind_str.to_string(),
                    name: name.clone(),
                    symbol_path: name.clone(),
                    parent_symbol: None,
                    visibility: detect_visibility(lang, sub_slice),
                    doc_comment: None,
                    signature: first_meaningful_line(sub_slice),
                    is_test: looks_like_test(lang, sub_slice, name.as_deref()),
                    is_public: false,
                    calls: extract_calls(sub_slice),
                    start_line: sl,
                    end_line: el,
                    byte_start: sub.0,
                    byte_end: sub.1,
                    content: sub_slice.to_string(),
                });
            }
            emitted_ranges.push((start, end));
        }
    }
    if chunks.is_empty() {
        return None;
    }
    Some(chunks)
}

fn split_on_blank_lines(
    content: &str,
    start: usize,
    end: usize,
    max_bytes: usize,
) -> Vec<(usize, usize)> {
    let slice = &content[start..end];
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut chunk_start = start;
    let mut cursor = start;
    let bytes = slice.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find next blank line break (\n\n).
        let abs = start + i;
        if bytes[i] == b'\n' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            if abs - chunk_start >= max_bytes {
                out.push((chunk_start, abs));
                chunk_start = abs + 1;
            }
            cursor = abs + 2;
        }
        i += 1;
    }
    let _ = cursor;
    if chunk_start < end {
        out.push((chunk_start, end));
    }
    // If no blank lines found, hard-split.
    if out.is_empty() || out.iter().any(|(s, e)| e - s > max_bytes * 2) {
        return hard_split(content, start, end, max_bytes);
    }
    out
}

fn hard_split(content: &str, start: usize, end: usize, max_bytes: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut s = start;
    while s < end {
        let mut e = (s + max_bytes).min(end);
        // Walk back to a char boundary.
        while e < end && !content.is_char_boundary(e) {
            e -= 1;
        }
        if e <= s {
            e = end;
        }
        out.push((s, e));
        s = e;
    }
    out
}

// ----- Fallback chunking ----------------------------------------------------

fn chunk_fallback(content: &str, max_bytes: usize) -> Vec<CodeChunk> {
    if content.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<CodeChunk> = Vec::new();
    let bytes = content.as_bytes();
    let mut s = 0usize;
    while s < bytes.len() {
        let mut e = (s + max_bytes).min(bytes.len());
        while e < bytes.len() && !content.is_char_boundary(e) {
            e -= 1;
        }
        // Prefer to end at a newline if one is within 100 bytes back.
        if e < bytes.len() {
            let lo = e.saturating_sub(100);
            if let Some(nl) = content[lo..e].rfind('\n') {
                e = lo + nl + 1;
            }
        }
        let (sl, el) = line_range_for(content, s, e);
        out.push(CodeChunk {
            kind: "fallback".to_string(),
            name: None,
            symbol_path: None,
            parent_symbol: None,
            visibility: None,
            doc_comment: None,
            signature: None,
            is_test: false,
            is_public: false,
            calls: Vec::new(),
            start_line: sl,
            end_line: el,
            byte_start: s,
            byte_end: e,
            content: content[s..e].to_string(),
        });
        s = e;
    }
    out
}

// ----- Helpers --------------------------------------------------------------

fn line_range_for(content: &str, start: usize, end: usize) -> (u32, u32) {
    let bytes = content.as_bytes();
    let mut line = 1u32;
    let mut start_line = 1u32;
    let mut end_line = 1u32;
    for (i, &b) in bytes.iter().enumerate() {
        if i == start {
            start_line = line;
        }
        if i >= end {
            end_line = line;
            return (start_line, end_line);
        }
        if b == b'\n' {
            line += 1;
        }
    }
    end_line = line;
    (start_line, end_line)
}

fn first_meaningful_line(s: &str) -> Option<String> {
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        return Some(t.chars().take(200).collect());
    }
    None
}

fn detect_visibility(lang: Lang, slice: &str) -> Option<String> {
    let t = slice.trim_start();
    match lang {
        Lang::Rust => {
            if t.starts_with("pub(crate)") {
                Some("pub(crate)".into())
            } else if t.starts_with("pub ") || t.starts_with("pub(") {
                Some("pub".into())
            } else {
                Some("priv".into())
            }
        }
        Lang::Ts | Lang::Tsx | Lang::Js | Lang::Jsx => {
            if t.starts_with("export ") {
                Some("exported".into())
            } else {
                None
            }
        }
        Lang::Python => None, // by convention: leading _ = private
        Lang::Go => {
            // Exported = starts with uppercase letter on the declaration name.
            // Cheap approximation: scan for first identifier-ish char.
            let head: String = t.chars().take(120).collect();
            let exported = head
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .find(|w| !w.is_empty() && *w != "func" && *w != "type")
                .map(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
                .unwrap_or(false);
            Some(if exported { "exported".into() } else { "priv".into() })
        }
        _ => None,
    }
}

fn looks_like_test(lang: Lang, slice: &str, name: Option<&str>) -> bool {
    match lang {
        Lang::Rust => slice.contains("#[test]") || slice.contains("#[tokio::test]"),
        Lang::Python => name.map(|n| n.starts_with("test_")).unwrap_or(false),
        Lang::Go => name.map(|n| n.starts_with("Test")).unwrap_or(false),
        Lang::Ts | Lang::Tsx | Lang::Js | Lang::Jsx => {
            slice.contains("describe(") || slice.contains("it(") || slice.contains("test(")
        }
        _ => false,
    }
}

fn extract_doc_comment(lang: Lang, content: &str, sym_start: usize) -> Option<String> {
    let head = &content[..sym_start];
    let mut lines: Vec<&str> = head.lines().rev().collect();
    let mut out: Vec<String> = Vec::new();
    match lang {
        Lang::Rust | Lang::Ts | Lang::Tsx | Lang::Js | Lang::Jsx | Lang::Go => {
            for l in lines.iter() {
                let t = l.trim();
                if t.is_empty() {
                    break;
                }
                if let Some(rest) = t.strip_prefix("///") {
                    out.push(rest.trim().to_string());
                } else if let Some(rest) = t.strip_prefix("//!") {
                    out.push(rest.trim().to_string());
                } else if let Some(rest) = t.strip_prefix("//") {
                    out.push(rest.trim().to_string());
                } else if t.starts_with("*") || t == "*/" {
                    out.push(t.trim_matches('*').trim().to_string());
                } else {
                    break;
                }
            }
        }
        Lang::Python => {
            // Python docstring is the first string literal *inside* the body;
            // approximated by next non-blank lines after the def — handled
            // elsewhere. Leave None here.
            let _ = &mut lines;
        }
        _ => {}
    }
    if out.is_empty() {
        return None;
    }
    out.reverse();
    Some(out.join("\n"))
}

/// Extract identifier-looking tokens immediately followed by `(`. Cheap
/// heuristic for "what does this chunk call". Used to power `code_find_callers`.
fn extract_calls(slice: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = slice.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() {
                let cc = bytes[i] as char;
                if cc.is_ascii_alphanumeric() || cc == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            // Skip any whitespace then check for `(`.
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'(' {
                let ident = &slice[start..i];
                if ident.len() >= 2 && !is_kw(ident) {
                    let s = ident.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

fn is_kw(s: &str) -> bool {
    matches!(
        s,
        "if" | "for"
            | "while"
            | "match"
            | "return"
            | "fn"
            | "let"
            | "mut"
            | "pub"
            | "use"
            | "mod"
            | "impl"
            | "struct"
            | "enum"
            | "trait"
            | "as"
            | "self"
            | "Self"
            | "true"
            | "false"
            | "in"
            | "ref"
            | "where"
            | "async"
            | "await"
            | "move"
            | "loop"
            | "break"
            | "continue"
            | "type"
            | "const"
            | "static"
            | "fn_"
            | "def"
            | "class"
            | "import"
            | "from"
            | "with"
            | "try"
            | "except"
            | "raise"
            | "yield"
            | "lambda"
            | "func"
            | "go"
            | "defer"
            | "package"
            | "var"
            | "switch"
            | "case"
            | "default"
    )
}

// ----- File-level extractors ------------------------------------------------

fn extract_summary(lang: Lang, content: &str) -> Option<String> {
    let mut lines = content.lines();
    let mut out: Vec<String> = Vec::new();
    let mut in_block = false;
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.is_empty() {
            if !out.is_empty() {
                break;
            } else {
                continue;
            }
        }
        match lang {
            Lang::Rust => {
                if let Some(rest) = t.strip_prefix("//!") {
                    out.push(rest.trim().to_string());
                } else if let Some(rest) = t.strip_prefix("///") {
                    out.push(rest.trim().to_string());
                } else if t.starts_with("//") {
                    out.push(t.trim_start_matches('/').trim().to_string());
                } else {
                    break;
                }
            }
            Lang::Ts | Lang::Tsx | Lang::Js | Lang::Jsx | Lang::Go => {
                if t.starts_with("/**") {
                    in_block = true;
                    let inner = t.trim_start_matches("/**").trim_end_matches("*/").trim();
                    if !inner.is_empty() {
                        out.push(inner.to_string());
                    }
                    if t.ends_with("*/") {
                        in_block = false;
                        break;
                    }
                } else if in_block {
                    let inner = t.trim_start_matches('*').trim();
                    if t.ends_with("*/") {
                        in_block = false;
                        if !inner.is_empty() {
                            out.push(inner.trim_end_matches("*/").trim().to_string());
                        }
                        break;
                    }
                    out.push(inner.to_string());
                } else if t.starts_with("//") {
                    out.push(t.trim_start_matches('/').trim().to_string());
                } else {
                    break;
                }
            }
            Lang::Python => {
                if t.starts_with("\"\"\"") || t.starts_with("'''") {
                    // Module docstring: consume until closing.
                    let quote = &t[..3];
                    let after = &t[3..];
                    if let Some(idx) = after.find(quote) {
                        return Some(after[..idx].trim().to_string());
                    }
                    out.push(after.to_string());
                    for line in lines.by_ref() {
                        if let Some(idx) = line.find(quote) {
                            out.push(line[..idx].to_string());
                            return Some(out.join("\n").trim().to_string());
                        }
                        out.push(line.to_string());
                    }
                    break;
                } else if t.starts_with('#') {
                    out.push(t.trim_start_matches('#').trim().to_string());
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join("\n"))
    }
}

fn extract_imports(lang: Lang, content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in content.lines().take(200) {
        let t = line.trim();
        match lang {
            Lang::Rust => {
                if let Some(rest) = t.strip_prefix("use ") {
                    if let Some(stripped) = rest.strip_suffix(';') {
                        out.push(stripped.trim().to_string());
                    }
                }
            }
            Lang::Ts | Lang::Tsx | Lang::Js | Lang::Jsx => {
                if t.starts_with("import ") {
                    if let Some(idx) = t.find(" from ") {
                        let q = t[idx + 6..].trim().trim_end_matches(';');
                        out.push(q.trim_matches(|c| c == '\'' || c == '"').to_string());
                    } else if let Some(rest) = t.strip_prefix("import ") {
                        out.push(
                            rest.trim()
                                .trim_end_matches(';')
                                .trim_matches(|c| c == '\'' || c == '"')
                                .to_string(),
                        );
                    }
                }
            }
            Lang::Python => {
                if let Some(rest) = t.strip_prefix("from ") {
                    if let Some(idx) = rest.find(" import") {
                        out.push(rest[..idx].trim().to_string());
                    }
                } else if let Some(rest) = t.strip_prefix("import ") {
                    out.push(rest.split([' ', ',']).next().unwrap_or("").to_string());
                }
            }
            Lang::Go => {
                if let Some(rest) = t.strip_prefix("import ") {
                    out.push(rest.trim_matches(|c: char| c == '"' || c.is_whitespace()).to_string());
                }
            }
            _ => {}
        }
    }
    out.retain(|s| !s.is_empty());
    out.sort();
    out.dedup();
    out
}

fn extract_todos(content: &str) -> Vec<TodoEntry> {
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        // Find where the line's comment context begins (if any) — the marker
        // must appear strictly after that point. Otherwise `TODO_count` etc.
        // would falsely match.
        let comment_pos = comment_start(line);
        let Some(cstart) = comment_pos else { continue };
        let body = &line[cstart..];
        for kind in ["TODO", "FIXME", "HACK", "XXX", "NOTE"] {
            if let Some(pos) = body.find(kind) {
                let after = &body[pos + kind.len()..];
                let text = after
                    .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
                    .trim_end()
                    .to_string();
                out.push(TodoEntry {
                    kind: kind.to_string(),
                    line: (idx + 1) as u32,
                    text,
                });
                break;
            }
        }
    }
    out
}

/// Walk `start` backward past any consecutive lines that look like
/// attributes / decorators belonging to the symbol that begins at `start`.
fn extend_start_for_decorations(lang: Lang, content: &str, start: usize) -> usize {
    let mut cur = start;
    loop {
        // Find the line that ends just before `cur`.
        let head = &content[..cur];
        let line_start = head
            .rfind('\n')
            .map(|p| {
                // Look at the line whose newline ends at `p`. Its content is
                // between the previous newline (or 0) and `p`.
                let prev = head[..p].rfind('\n').map(|q| q + 1).unwrap_or(0);
                (prev, p)
            })
            .unwrap_or((0, head.len()));
        let line = &content[line_start.0..line_start.1];
        let t = line.trim();
        let is_decoration = match lang {
            Lang::Rust => t.starts_with("#[") || t.starts_with("#!["),
            Lang::Python => t.starts_with("@"),
            _ => false,
        };
        if !is_decoration || line_start.0 >= cur {
            return cur;
        }
        cur = line_start.0;
    }
}

fn comment_start(line: &str) -> Option<usize> {
    let trimmed_offset = line.len() - line.trim_start().len();
    let trimmed = &line[trimmed_offset..];
    if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("*")
        || trimmed.starts_with("/*")
    {
        return Some(trimmed_offset);
    }
    // Trailing comment after code.
    if let Some(p) = line.find("//") {
        return Some(p);
    }
    if let Some(p) = line.find("/*") {
        return Some(p);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_chunks_unknown_lang() {
        let src = "line a\nline b\nline c\n".repeat(50);
        let r = analyze_file("notes.txt", Lang::Other, &src, 200);
        assert!(!r.chunks.is_empty());
        for c in &r.chunks {
            assert_eq!(c.kind, "fallback");
            assert!(c.byte_end > c.byte_start);
            assert!(c.end_line >= c.start_line);
        }
    }

    #[test]
    fn rust_ast_emits_function_chunks() {
        let src = r#"
//! mod doc here.

use std::path::Path;

/// adds.
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[test]
fn t_add() { assert_eq!(add(1, 2), 3); }
"#;
        let r = analyze_file("src/lib.rs", Lang::Rust, src, 4096);
        assert!(r.chunks.iter().any(|c| c.name.as_deref() == Some("add")));
        let add_chunk = r.chunks.iter().find(|c| c.name.as_deref() == Some("add")).unwrap();
        assert!(add_chunk.is_public);
        assert_eq!(add_chunk.visibility.as_deref(), Some("pub"));
        assert!(add_chunk.doc_comment.as_deref().unwrap_or("").contains("adds"));
        let t = r.chunks.iter().find(|c| c.name.as_deref() == Some("t_add")).unwrap();
        assert!(t.is_test);
        assert!(r.summary.as_deref().unwrap_or("").contains("mod doc"));
        assert!(r.imports.iter().any(|i| i == "std::path::Path"));
        assert_eq!(r.role.as_deref(), Some("lib"));
    }

    #[test]
    fn extracts_todos_only_in_comments() {
        let src = "let TODO_count = 1; // TODO: rename me\n# FIXME: bad\nlet x = 'TODO not me';\n";
        let todos = extract_todos(src);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].kind, "TODO");
        assert_eq!(todos[1].kind, "FIXME");
    }

    #[test]
    fn detect_role_buckets() {
        assert_eq!(detect_role("tests/foo.rs"), Some("test"));
        assert_eq!(detect_role("examples/x.rs"), Some("example"));
        assert_eq!(detect_role("src/main.rs"), Some("bin"));
        assert_eq!(detect_role("README.md"), Some("doc"));
        assert_eq!(detect_role("Cargo.toml"), Some("build"));
        assert_eq!(detect_role("src/lib.rs"), Some("lib"));
    }
}

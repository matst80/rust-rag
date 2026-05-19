//! Google Drive read operations on top of `GoogleClient`.
//!
//! Scope: `https://www.googleapis.com/auth/drive.readonly` (granted by the
//! default scope set in `GoogleOAuthConfig`).
//!
//! API reference: https://developers.google.com/workspace/drive/api/reference/rest/v3/files

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::client::{GoogleClient, GoogleClientError};

const FILES_LIST: &str = "https://www.googleapis.com/drive/v3/files";
const FILES_GET: &str = "https://www.googleapis.com/drive/v3/files";
const DOC_MIME: &str = "application/vnd.google-apps.document";
const SHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";
const SLIDES_MIME: &str = "application/vnd.google-apps.presentation";

/// Maximum bytes returned by `fetch` to keep MCP responses bounded.
/// Larger files are truncated and the response flags `truncated: true`.
const MAX_FETCH_BYTES: usize = 200_000;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    #[serde(rename = "modifiedTime", skip_serializing_if = "Option::is_none")]
    pub modified_time: Option<String>,
    #[serde(rename = "webViewLink", skip_serializing_if = "Option::is_none")]
    pub web_view_link: Option<String>,
    #[serde(rename = "owners", skip_serializing_if = "Option::is_none")]
    pub owners: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct FileListResponse {
    files: Vec<DriveFile>,
    #[serde(rename = "nextPageToken")]
    #[allow(dead_code)]
    next_page_token: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchResult {
    pub files: Vec<DriveFile>,
    /// The literal `q` passed to the Drive API (post-escaping).
    pub query: String,
}

/// Search Drive. `query` is a free-text query that matches against file
/// names and full-text contents (Drive API `fullText contains` operator).
/// Pass `mime_type` to constrain results (e.g. only Google Docs).
pub async fn search(
    client: &GoogleClient,
    query: &str,
    page_size: u32,
    mime_type: Option<&str>,
) -> Result<SearchResult, GoogleClientError> {
    let page_size = page_size.clamp(1, 100);
    let escaped = escape_q_string(query);
    let mut q = format!("fullText contains '{escaped}' and trashed = false");
    if let Some(mt) = mime_type {
        let mt = escape_q_string(mt);
        q.push_str(&format!(" and mimeType = '{mt}'"));
    }

    let req = client.get(FILES_LIST).query(&[
        ("q", q.as_str()),
        ("pageSize", &page_size.to_string()),
        (
            "fields",
            "files(id,name,mimeType,modifiedTime,webViewLink,owners),nextPageToken",
        ),
        ("orderBy", "modifiedTime desc"),
        ("corpora", "user"),
    ]);
    let resp: FileListResponse = client.get_json(req).await?;
    Ok(SearchResult {
        files: resp.files,
        query: q,
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FetchedDoc {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    /// MIME type of the returned `content` (e.g. `text/markdown` for
    /// exported Docs, `text/plain` for native text files).
    pub returned_mime: String,
    /// UTF-8 decoded body. Binary files are NOT returned — fetch is
    /// intended for documents; use the raw API for blobs.
    pub content: String,
    pub truncated: bool,
    pub size_bytes: usize,
    pub web_view_link: Option<String>,
}

/// Fetch a single Drive file by id. Google Docs/Sheets/Slides are exported
/// via `files.export` (Markdown for Docs, TSV for Sheets, plain text for
/// Slides). Other text-y MIME types are downloaded via `files.get?alt=media`.
/// Binary types return an error — use a dedicated binary endpoint if needed.
pub async fn fetch(client: &GoogleClient, file_id: &str) -> Result<FetchedDoc, GoogleClientError> {
    // First fetch metadata so we know the mime type.
    let meta_url = format!("{FILES_GET}/{file_id}");
    let meta_req = client.get(&meta_url).query(&[(
        "fields",
        "id,name,mimeType,modifiedTime,webViewLink,owners",
    )]);
    let meta: DriveFile = client.get_json(meta_req).await?;

    let (returned_mime, bytes) = match meta.mime_type.as_str() {
        DOC_MIME => {
            // Google Docs export: prefer Markdown (smaller, structured).
            let req = client
                .get(&format!("{FILES_GET}/{file_id}/export"))
                .query(&[("mimeType", "text/markdown")]);
            let (_, body) = client.get_bytes(req).await?;
            ("text/markdown".to_string(), body)
        }
        SHEET_MIME => {
            // Sheets: TSV is the most readable text export.
            let req = client
                .get(&format!("{FILES_GET}/{file_id}/export"))
                .query(&[("mimeType", "text/tab-separated-values")]);
            let (_, body) = client.get_bytes(req).await?;
            ("text/tab-separated-values".to_string(), body)
        }
        SLIDES_MIME => {
            let req = client
                .get(&format!("{FILES_GET}/{file_id}/export"))
                .query(&[("mimeType", "text/plain")]);
            let (_, body) = client.get_bytes(req).await?;
            ("text/plain".to_string(), body)
        }
        other if is_textual_mime(other) => {
            let req = client
                .get(&format!("{FILES_GET}/{file_id}"))
                .query(&[("alt", "media")]);
            let (_, body) = client.get_bytes(req).await?;
            (other.to_string(), body)
        }
        other => {
            return Err(GoogleClientError::Other(anyhow::anyhow!(
                "mime type {other} is not text-fetchable via drive_fetch — \
                 only google docs/sheets/slides and text/* are supported"
            )));
        }
    };

    let size_bytes = bytes.len();
    let (content_bytes, truncated) = if size_bytes > MAX_FETCH_BYTES {
        (&bytes[..MAX_FETCH_BYTES], true)
    } else {
        (bytes.as_slice(), false)
    };
    let content = String::from_utf8_lossy(content_bytes).into_owned();

    Ok(FetchedDoc {
        id: meta.id,
        name: meta.name,
        mime_type: meta.mime_type,
        returned_mime,
        content,
        truncated,
        size_bytes,
        web_view_link: meta.web_view_link,
    })
}

fn is_textual_mime(m: &str) -> bool {
    m.starts_with("text/")
        || matches!(
            m,
            "application/json" | "application/xml" | "application/x-yaml"
        )
}

/// Escape user input for inclusion in a Drive `q` string literal. The Drive
/// API uses single-quoted strings; backslash-escape `\` and `'`.
fn escape_q_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_q_handles_quotes_and_backslashes() {
        assert_eq!(escape_q_string("o'reilly"), "o\\'reilly");
        assert_eq!(escape_q_string("a\\b"), "a\\\\b");
        assert_eq!(escape_q_string("plain"), "plain");
    }
}

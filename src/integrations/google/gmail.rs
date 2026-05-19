//! Gmail read operations on top of `GoogleClient`.
//!
//! Scope: `https://www.googleapis.com/auth/gmail.readonly`.
//!
//! API reference: https://developers.google.com/gmail/api/reference/rest/v1/users.messages
//!
//! Body decoding strategy: prefer `text/plain` parts; fall back to converting
//! `text/html` via `html2md` (already a dep). All Gmail bodies arrive as
//! URL-safe base64 in `body.data`.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::client::{GoogleClient, GoogleClientError};

const MESSAGES_LIST: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const THREADS_GET: &str = "https://gmail.googleapis.com/gmail/v1/users/me/threads";

/// Per-message body cap. Most personal/business mail fits in this; CI digests
/// and newsletters get clipped.
const MAX_BODY_BYTES: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MessageSummary {
    pub id: String,
    pub thread_id: String,
    pub snippet: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub subject: Option<String>,
    pub date_iso: Option<String>,
    pub label_ids: Vec<String>,
    pub has_attachment: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchResult {
    pub messages: Vec<MessageSummary>,
    /// Gmail's `nextPageToken`. Pass back as `page_token` to paginate.
    pub next_page_token: Option<String>,
    /// Approximate result count Gmail reports (server-side estimate).
    pub estimated_total: Option<u64>,
    pub query: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ThreadMessage {
    pub id: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub cc: Option<String>,
    pub subject: Option<String>,
    pub date_iso: Option<String>,
    /// Decoded body. Prefers `text/plain`, falls back to `html2md` of
    /// `text/html`. Empty when neither part exists (rare).
    pub body_text: String,
    pub body_truncated: bool,
    pub body_source: BodySource,
    pub label_ids: Vec<String>,
    pub has_attachment: bool,
}

#[derive(Debug, Serialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BodySource {
    PlainText,
    HtmlConverted,
    None,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FetchedThread {
    pub id: String,
    pub history_id: Option<String>,
    pub messages: Vec<ThreadMessage>,
}

/// Search Gmail using the standard query operators
/// (https://support.google.com/mail/answer/7190). Returns up to `page_size`
/// (1-100, default 20) summaries with the most common headers pre-extracted.
pub async fn search(
    client: &GoogleClient,
    query: &str,
    page_size: u32,
    page_token: Option<&str>,
) -> Result<SearchResult, GoogleClientError> {
    let page_size = page_size.clamp(1, 100).to_string();

    // First, list message ids matching the query. Gmail's list endpoint only
    // returns ids + thread_ids; we have to follow up with `messages.get` per
    // result to pull headers and snippet.
    #[derive(Deserialize)]
    struct ListResponse {
        #[serde(default)]
        messages: Vec<MessageRef>,
        #[serde(rename = "nextPageToken", default)]
        next_page_token: Option<String>,
        #[serde(rename = "resultSizeEstimate", default)]
        result_size_estimate: Option<u64>,
    }
    #[derive(Deserialize)]
    struct MessageRef {
        id: String,
        #[allow(dead_code)]
        #[serde(rename = "threadId")]
        thread_id: String,
    }

    let mut req = client
        .get(MESSAGES_LIST)
        .query(&[("q", query), ("maxResults", page_size.as_str())]);
    if let Some(tok) = page_token {
        req = req.query(&[("pageToken", tok)]);
    }
    let listing: ListResponse = client.get_json(req).await?;

    // Fan-out metadata fetches. Gmail's rate limits per project are generous
    // for read-only metadata calls; we cap at the page size we already
    // clamped, so this is bounded.
    let mut summaries = Vec::with_capacity(listing.messages.len());
    for m in listing.messages {
        match fetch_message_metadata(client, &m.id).await {
            Ok(s) => summaries.push(s),
            Err(e) => {
                tracing::warn!(message_id = %m.id, error = %e, "gmail_search: skip message after metadata fetch error");
            }
        }
    }

    Ok(SearchResult {
        messages: summaries,
        next_page_token: listing.next_page_token,
        estimated_total: listing.result_size_estimate,
        query: query.to_owned(),
    })
}

async fn fetch_message_metadata(
    client: &GoogleClient,
    id: &str,
) -> Result<MessageSummary, GoogleClientError> {
    let url = format!("{MESSAGES_LIST}/{id}");
    let req = client.get(&url).query(&[
        ("format", "metadata"),
        ("metadataHeaders", "From"),
        ("metadataHeaders", "To"),
        ("metadataHeaders", "Subject"),
        ("metadataHeaders", "Date"),
    ]);
    let raw: RawMessage = client.get_json(req).await?;
    let headers = raw.payload.as_ref().map(|p| &p.headers[..]).unwrap_or(&[]);
    let has_attachment = raw
        .payload
        .as_ref()
        .map(|p| payload_has_attachment(p))
        .unwrap_or(false);
    Ok(MessageSummary {
        id: raw.id,
        thread_id: raw.thread_id,
        snippet: raw.snippet,
        from: header_value(headers, "From"),
        to: header_value(headers, "To"),
        subject: header_value(headers, "Subject"),
        date_iso: header_value(headers, "Date"),
        label_ids: raw.label_ids.unwrap_or_default(),
        has_attachment,
    })
}

/// Fetch every message in a thread with decoded bodies.
pub async fn get_thread(
    client: &GoogleClient,
    thread_id: &str,
) -> Result<FetchedThread, GoogleClientError> {
    let url = format!("{THREADS_GET}/{thread_id}");
    let req = client.get(&url).query(&[("format", "full")]);
    #[derive(Deserialize)]
    struct ThreadResponse {
        id: String,
        #[serde(rename = "historyId")]
        history_id: Option<String>,
        #[serde(default)]
        messages: Vec<RawMessage>,
    }
    let resp: ThreadResponse = client.get_json(req).await?;
    let messages = resp.messages.into_iter().map(to_thread_message).collect();
    Ok(FetchedThread {
        id: resp.id,
        history_id: resp.history_id,
        messages,
    })
}

fn to_thread_message(raw: RawMessage) -> ThreadMessage {
    let headers = raw
        .payload
        .as_ref()
        .map(|p| &p.headers[..])
        .unwrap_or(&[]);
    let from = header_value(headers, "From");
    let to = header_value(headers, "To");
    let cc = header_value(headers, "Cc");
    let subject = header_value(headers, "Subject");
    let date_iso = header_value(headers, "Date");

    let (body_text, body_source, body_truncated) = raw
        .payload
        .as_ref()
        .map(extract_best_body)
        .unwrap_or((String::new(), BodySource::None, false));

    let has_attachment = raw
        .payload
        .as_ref()
        .map(|p| payload_has_attachment(p))
        .unwrap_or(false);

    ThreadMessage {
        id: raw.id,
        from,
        to,
        cc,
        subject,
        date_iso,
        body_text,
        body_truncated,
        body_source,
        label_ids: raw.label_ids.unwrap_or_default(),
        has_attachment,
    }
}

#[derive(Deserialize, Debug)]
struct RawMessage {
    id: String,
    #[serde(rename = "threadId")]
    thread_id: String,
    #[serde(default)]
    snippet: Option<String>,
    #[serde(rename = "labelIds", default)]
    label_ids: Option<Vec<String>>,
    payload: Option<MessagePart>,
}

#[derive(Deserialize, Debug)]
struct MessagePart {
    #[serde(rename = "mimeType", default)]
    mime_type: Option<String>,
    #[serde(rename = "filename", default)]
    filename: Option<String>,
    #[serde(default)]
    headers: Vec<Header>,
    body: Option<PartBody>,
    #[serde(default)]
    parts: Vec<MessagePart>,
}

#[derive(Deserialize, Debug)]
struct PartBody {
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    size: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    attachment_id: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Header {
    name: String,
    value: String,
}

fn header_value(headers: &[Header], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.clone())
}

/// Walk the MIME tree and return the best textual body we can extract.
/// Preference: first `text/plain` we encounter, otherwise convert the first
/// `text/html` via `html2md`.
fn extract_best_body(payload: &MessagePart) -> (String, BodySource, bool) {
    // Pass 1: any text/plain anywhere in the tree.
    if let Some(plain) = find_part_text(payload, "text/plain") {
        let (trimmed, truncated) = clamp(&plain, MAX_BODY_BYTES);
        return (trimmed, BodySource::PlainText, truncated);
    }
    // Pass 2: text/html → markdown.
    if let Some(html) = find_part_text(payload, "text/html") {
        let md = html2md::parse_html(&html);
        let (trimmed, truncated) = clamp(&md, MAX_BODY_BYTES);
        return (trimmed, BodySource::HtmlConverted, truncated);
    }
    (String::new(), BodySource::None, false)
}

fn find_part_text(part: &MessagePart, want_mime: &str) -> Option<String> {
    if part.mime_type.as_deref() == Some(want_mime) {
        if let Some(body) = &part.body
            && let Some(data) = &body.data
            && let Some(decoded) = decode_b64url(data)
        {
            return Some(decoded);
        }
    }
    for sub in &part.parts {
        if let Some(found) = find_part_text(sub, want_mime) {
            return Some(found);
        }
    }
    None
}

fn payload_has_attachment(part: &MessagePart) -> bool {
    if part
        .filename
        .as_deref()
        .map(|n| !n.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    part.parts.iter().any(payload_has_attachment)
}

fn decode_b64url(s: &str) -> Option<String> {
    // Gmail uses URL-safe base64 sometimes with padding stripped. Tolerate
    // both by stripping padding and decoding with the no-pad engine.
    let trimmed = s.trim_end_matches('=');
    let bytes = URL_SAFE_NO_PAD.decode(trimmed).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn clamp(s: &str, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s.to_owned(), false);
    }
    // Cut on a char boundary near max_bytes to avoid panicking on UTF-8.
    let mut cut = max_bytes;
    while !s.is_char_boundary(cut) && cut > 0 {
        cut -= 1;
    }
    (s[..cut].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_url_safe_base64_with_or_without_padding() {
        // "hello" → aGVsbG8 (url-safe, no padding) / aGVsbG8= (with padding)
        assert_eq!(decode_b64url("aGVsbG8").as_deref(), Some("hello"));
        assert_eq!(decode_b64url("aGVsbG8=").as_deref(), Some("hello"));
    }

    #[test]
    fn clamp_respects_char_boundaries() {
        let s = "abc😀def"; // emoji is 4 bytes
        let (out, truncated) = clamp(s, 4);
        assert!(truncated);
        // Should not split the emoji — cut lands at byte 3 ("abc").
        assert_eq!(out, "abc");
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let h = vec![
            Header {
                name: "From".into(),
                value: "a@b".into(),
            },
            Header {
                name: "subject".into(),
                value: "hi".into(),
            },
        ];
        assert_eq!(header_value(&h, "from"), Some("a@b".into()));
        assert_eq!(header_value(&h, "SUBJECT"), Some("hi".into()));
    }
}

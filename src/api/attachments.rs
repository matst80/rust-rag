//! Attachments + wiki-tree HTTP routes.
//!
//! Files bind to existing entries. Disk lives under `state.upload_path`,
//! served read-only via the `/assets/*` route mounted on the outer router.
//!
//! `attach_from_url` fetches a remote URL server-side. SSRF-guarded: scheme
//! allowlist, private-IP block, byte/time caps, redirect re-check.

use axum::{
    Json,
    body::Bytes,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path as StdPath,
    time::Duration,
};
use tokio::fs;
use uuid::Uuid;

use super::{ApiError, AppState};
use crate::db::AttachmentRecord;

const DEFAULT_ATTACHMENT_MAX_BYTES: u64 = 25 * 1024 * 1024;
const ATTACHMENT_FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const ATTACHMENT_MAX_REDIRECTS: usize = 3;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentSummary {
    pub id: String,
    pub item_id: String,
    pub filename: Option<String>,
    pub stored_name: String,
    pub url: String,
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub sha256: Option<String>,
    pub created_at: i64,
}

impl From<AttachmentRecord> for AttachmentSummary {
    fn from(r: AttachmentRecord) -> Self {
        let url = format!("/assets/{}", r.stored_name);
        AttachmentSummary {
            id: r.id,
            item_id: r.item_id,
            filename: r.filename,
            stored_name: r.stored_name,
            url,
            mime: r.mime,
            size: r.size,
            sha256: r.sha256,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentsResponse {
    pub attachments: Vec<AttachmentSummary>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AttachUrlRequest {
    pub item_id: String,
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EntriesTreeQuery {
    pub source_id: String,
    #[serde(default)]
    pub prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TreeChild {
    pub segment: String,
    pub path: String,
    pub count: i64,
    pub has_children: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EntriesTreeResponse {
    pub source_id: String,
    pub prefix: Option<String>,
    pub children: Vec<TreeChild>,
    pub entries: Vec<super::AdminItemPayload>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EntriesPathsQuery {
    /// Optional namespace filter. Omit for every source_id at once.
    #[serde(default)]
    pub source_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PathRowPayload {
    pub source_id: String,
    pub path: String,
    pub count: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EntriesPathsResponse {
    pub paths: Vec<PathRowPayload>,
}

fn attachment_max_bytes() -> u64 {
    std::env::var("RAG_ATTACHMENT_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_ATTACHMENT_MAX_BYTES)
}

fn extension_from_filename(name: &str) -> Option<String> {
    StdPath::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
}

fn extension_from_mime(mime: &str) -> Option<&'static str> {
    match mime.split(';').next().unwrap_or(mime).trim() {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "application/pdf" => Some("pdf"),
        "application/json" => Some("json"),
        "text/plain" => Some("txt"),
        "text/markdown" => Some("md"),
        "text/csv" => Some("csv"),
        "text/html" => Some("html"),
        "application/zip" => Some("zip"),
        "application/x-tar" => Some("tar"),
        "application/gzip" => Some("gz"),
        _ => None,
    }
}

fn sanitize_filename(name: &str) -> String {
    let candidate = StdPath::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let cleaned: String = candidate
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect();
    if cleaned.trim().is_empty() {
        "file".to_owned()
    } else {
        cleaned
    }
}

async fn ensure_item_exists(state: &AppState, item_id: &str) -> Result<(), ApiError> {
    let store = state.store.clone();
    let id = item_id.to_owned();
    let exists = tokio::task::spawn_blocking(move || store.get_item(&id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    if exists.is_none() {
        return Err(ApiError::NotFound(format!("item {item_id} not found")));
    }
    Ok(())
}

async fn write_to_disk(
    state: &AppState,
    bytes: &Bytes,
    extension: &str,
) -> Result<String, ApiError> {
    let stored_name = if extension.is_empty() {
        Uuid::now_v7().to_string()
    } else {
        format!("{}.{}", Uuid::now_v7(), extension)
    };
    let dir = state.upload_path.as_str();
    fs::create_dir_all(dir)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("create_dir_all {dir}: {e}")))?;
    fs::write(format!("{dir}/{stored_name}"), bytes.as_ref())
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("write {stored_name}: {e}")))?;
    Ok(stored_name)
}

fn now_ms() -> Result<i64, ApiError> {
    super::current_timestamp_millis()
}

async fn persist_record(
    state: &AppState,
    item_id: String,
    filename: Option<String>,
    stored_name: String,
    mime: Option<String>,
    size: Option<i64>,
    sha256: Option<String>,
) -> Result<AttachmentRecord, ApiError> {
    let id = Uuid::now_v7().to_string();
    let record = AttachmentRecord {
        id: id.clone(),
        item_id,
        filename,
        stored_name,
        mime,
        size,
        sha256,
        created_at: now_ms()?,
    };
    let store = state.store.clone();
    let to_persist = record.clone();
    tokio::task::spawn_blocking(move || store.insert_attachment(to_persist))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    Ok(record)
}

pub async fn upload_multipart(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<AttachmentSummary>), ApiError> {
    let max = attachment_max_bytes();
    let mut item_id: Option<String> = None;
    let mut file_bytes: Option<Bytes> = None;
    let mut filename: Option<String> = None;
    let mut mime: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_owned();
        match name.as_str() {
            "item_id" => {
                item_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(e.to_string()))?,
                );
            }
            "file" => {
                filename = field.file_name().map(ToOwned::to_owned);
                mime = field.content_type().map(ToOwned::to_owned);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                if bytes.len() as u64 > max {
                    return Err(ApiError::BadRequest(format!(
                        "file exceeds RAG_ATTACHMENT_MAX_BYTES ({max} bytes)"
                    )));
                }
                file_bytes = Some(bytes);
            }
            _ => {}
        }
    }

    let item_id = item_id.ok_or_else(|| ApiError::BadRequest("missing item_id".to_owned()))?;
    let bytes = file_bytes.ok_or_else(|| ApiError::BadRequest("missing file".to_owned()))?;

    ensure_item_exists(&state, &item_id).await?;

    let safe_name = filename.as_deref().map(sanitize_filename);
    let extension = safe_name
        .as_deref()
        .and_then(extension_from_filename)
        .or_else(|| mime.as_deref().and_then(extension_from_mime).map(str::to_owned))
        .unwrap_or_default();
    let stored_name = write_to_disk(&state, &bytes, &extension).await?;
    let sha = format!("{:x}", Sha256::digest(&bytes));
    let size = i64::try_from(bytes.len()).ok();
    let record = persist_record(
        &state,
        item_id,
        safe_name,
        stored_name,
        mime,
        size,
        Some(sha),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(record.into())))
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => is_private_v6(v6),
    }
}

fn is_private_v4(ip: &Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        // 100.64.0.0/10 carrier-grade NAT
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64)
        // 169.254.0.0/16 link-local already via is_link_local
        // 192.0.0.0/24 IETF protocol assignments
        || (ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 0)
        // 198.18.0.0/15 benchmarking
        || (ip.octets()[0] == 198 && (ip.octets()[1] & 0xfe) == 18)
}

fn is_private_v6(ip: &Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // unique local fc00::/7
        || (ip.segments()[0] & 0xfe00) == 0xfc00
        // link-local fe80::/10
        || (ip.segments()[0] & 0xffc0) == 0xfe80
        // ipv4-mapped: re-check the v4 portion
        || matches!(ip.to_ipv4_mapped(), Some(v4) if is_private_v4(&v4))
}

async fn safe_fetch(url_str: &str, max_bytes: u64) -> Result<(Bytes, Option<String>), ApiError> {
    let mut current = reqwest::Url::parse(url_str)
        .map_err(|e| ApiError::BadRequest(format!("invalid url: {e}")))?;
    let client = reqwest::Client::builder()
        .timeout(ATTACHMENT_FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiError::Internal(e.into()))?;

    for _ in 0..=ATTACHMENT_MAX_REDIRECTS {
        match current.scheme() {
            "http" | "https" => {}
            other => {
                return Err(ApiError::BadRequest(format!(
                    "scheme '{other}' not allowed; use http or https"
                )));
            }
        }
        let host = current
            .host_str()
            .ok_or_else(|| ApiError::BadRequest("missing host".to_owned()))?;
        let port = current.port_or_known_default().unwrap_or(0);
        let lookup = tokio::net::lookup_host(format!("{host}:{port}"))
            .await
            .map_err(|e| ApiError::BadRequest(format!("dns lookup failed: {e}")))?;
        for sock in lookup {
            if is_private_ip(&sock.ip()) {
                return Err(ApiError::BadRequest(format!(
                    "host {host} resolves to private/loopback address"
                )));
            }
        }

        let resp = client
            .get(current.clone())
            .header("Accept", "text/markdown, text/html;q=0.9, application/xhtml+xml;q=0.9, */*;q=0.8")
            .send()
            .await
            .map_err(|e| ApiError::BadRequest(format!("fetch failed: {e}")))?;
        let status = resp.status();
        if status.is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ApiError::BadRequest("redirect missing Location".to_owned()))?;
            current = current
                .join(location)
                .map_err(|e| ApiError::BadRequest(format!("bad redirect target: {e}")))?;
            continue;
        }
        if !status.is_success() {
            return Err(ApiError::BadRequest(format!(
                "remote returned {status}"
            )));
        }
        if let Some(len) = resp.content_length() {
            if len > max_bytes {
                return Err(ApiError::BadRequest(format!(
                    "remote content-length {len} exceeds cap {max_bytes}"
                )));
            }
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        let body = resp
            .bytes()
            .await
            .map_err(|e| ApiError::BadRequest(format!("read body: {e}")))?;
        if body.len() as u64 > max_bytes {
            return Err(ApiError::BadRequest(format!(
                "downloaded {} bytes exceeds cap {max_bytes}",
                body.len()
            )));
        }
        return Ok((body, mime));
    }
    Err(ApiError::BadRequest("too many redirects".to_owned()))
}

pub async fn attach_from_url_core(
    state: &AppState,
    request: AttachUrlRequest,
) -> Result<AttachmentSummary, ApiError> {
    ensure_item_exists(state, &request.item_id).await?;
    let max = attachment_max_bytes();
    let (bytes, mime) = safe_fetch(&request.url, max).await?;

    let filename = request
        .filename
        .as_deref()
        .map(sanitize_filename)
        .or_else(|| {
            reqwest::Url::parse(&request.url).ok().and_then(|u| {
                u.path_segments()
                    .and_then(|mut s| s.next_back().map(ToOwned::to_owned))
                    .filter(|s| !s.is_empty())
                    .map(|s| sanitize_filename(&s))
            })
        });
    let extension = filename
        .as_deref()
        .and_then(extension_from_filename)
        .or_else(|| mime.as_deref().and_then(extension_from_mime).map(str::to_owned))
        .unwrap_or_default();
    let stored_name = write_to_disk(state, &bytes, &extension).await?;
    let sha = format!("{:x}", Sha256::digest(&bytes));
    let size = i64::try_from(bytes.len()).ok();
    let record =
        persist_record(state, request.item_id, filename, stored_name, mime, size, Some(sha))
            .await?;
    Ok(record.into())
}

pub async fn attach_from_url(
    State(state): State<AppState>,
    Json(request): Json<AttachUrlRequest>,
) -> Result<(StatusCode, Json<AttachmentSummary>), ApiError> {
    let summary = attach_from_url_core(&state, request).await?;
    Ok((StatusCode::CREATED, Json(summary)))
}

pub async fn list_for_item(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
) -> Result<Json<AttachmentsResponse>, ApiError> {
    let store = state.store.clone();
    let id = item_id.clone();
    let records = tokio::task::spawn_blocking(move || store.list_attachments_for_item(&id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    Ok(Json(AttachmentsResponse {
        attachments: records.into_iter().map(Into::into).collect(),
    }))
}

pub async fn delete_attachment_core(state: &AppState, id: &str) -> Result<(), ApiError> {
    let store = state.store.clone();
    let lookup_id = id.to_owned();
    let stored_name = tokio::task::spawn_blocking(move || store.delete_attachment(&lookup_id))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    let Some(stored_name) = stored_name else {
        return Err(ApiError::NotFound(format!("attachment {id} not found")));
    };
    let path = format!("{}/{stored_name}", state.upload_path.as_str());
    if let Err(e) = fs::remove_file(&path).await {
        tracing::warn!(path = %path, error = %e, "attachment file delete failed");
    }
    Ok(())
}

pub async fn delete_attachment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    delete_attachment_core(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn entries_tree_core(
    state: &AppState,
    query: EntriesTreeQuery,
) -> Result<EntriesTreeResponse, ApiError> {
    super::validate_source_id(&query.source_id)?;
    let prefix_norm = match query.prefix.as_deref() {
        Some(p) => crate::db::normalize_path(p)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?,
        None => None,
    };

    let store = state.store.clone();
    let source_id = query.source_id.clone();
    let prefix_for_children = prefix_norm.clone();
    let children = tokio::task::spawn_blocking(move || {
        store.list_path_children(&source_id, prefix_for_children.as_deref())
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    // Entries whose path equals the prefix (leaf nodes at this level).
    let entries = if let Some(prefix) = prefix_norm.clone() {
        let store = state.store.clone();
        let req = crate::db::ListItemsRequest {
            source_id: Some(query.source_id.clone()),
            limit: Some(200),
            offset: None,
            sort_order: crate::db::SortOrder::Desc,
            metadata_filter: Default::default(),
            min_created_at: None,
            max_created_at: None,
            path_prefix: Some(prefix.clone()),
            type_name: None,
        };
        let want = prefix.to_lowercase();
        let (items, _) = tokio::task::spawn_blocking(move || store.list_items(req))
            .await
            .map_err(ApiError::TaskJoin)?
            .map_err(ApiError::Internal)?;
        items
            .into_iter()
            .filter(|i| i.path.as_deref().map(str::to_lowercase) == Some(want.clone()))
            .map(Into::into)
            .collect()
    } else {
        Vec::new()
    };

    let prefix_disp = prefix_norm.clone();
    let children = children
        .into_iter()
        .map(|c| {
            let path = match &prefix_disp {
                Some(p) => format!("{p}/{}", c.segment),
                None => c.segment.clone(),
            };
            TreeChild {
                segment: c.segment,
                path,
                count: c.count,
                has_children: c.has_children,
            }
        })
        .collect();

    Ok(EntriesTreeResponse {
        source_id: query.source_id,
        prefix: prefix_norm,
        children,
        entries,
    })
}

pub async fn entries_tree(
    State(state): State<AppState>,
    Query(query): Query<EntriesTreeQuery>,
) -> Result<Json<EntriesTreeResponse>, ApiError> {
    entries_tree_core(&state, query).await.map(Json)
}

pub async fn entries_paths_core(
    state: &AppState,
    query: EntriesPathsQuery,
) -> Result<EntriesPathsResponse, ApiError> {
    if let Some(s) = query.source_id.as_deref() {
        super::validate_source_id(s)?;
    }
    let store = state.store.clone();
    let filter = query.source_id.clone();
    let rows = tokio::task::spawn_blocking(move || store.list_all_paths(filter.as_deref()))
        .await
        .map_err(ApiError::TaskJoin)?
        .map_err(ApiError::Internal)?;
    Ok(EntriesPathsResponse {
        paths: rows
            .into_iter()
            .map(|r| PathRowPayload {
                source_id: r.source_id,
                path: r.path,
                count: r.count,
            })
            .collect(),
    })
}

pub async fn entries_paths(
    State(state): State<AppState>,
    Query(query): Query<EntriesPathsQuery>,
) -> Result<Json<EntriesPathsResponse>, ApiError> {
    entries_paths_core(&state, query).await.map(Json)
}

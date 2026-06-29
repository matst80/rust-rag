use axum::{Json, extract::State};

use crate::db::{ItemRecord, ListItemsRequest};

use super::error::ApiError;
use super::state::AppState;

pub(crate) async fn rebuild_map(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .projection_worker
        .run_rebuild(state.http_client.clone(), state.analysis.clone())
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(serde_json::json!({ "status": "started" })))
}

pub(crate) async fn get_map(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::projection::MapPoint>>, ApiError> {
    Ok(Json(build_map_points(state).await?))
}

pub async fn build_map_points(
    state: AppState,
) -> Result<Vec<crate::projection::MapPoint>, ApiError> {
    let (items, _) = tokio::task::spawn_blocking(move || {
        state.store.list_items(ListItemsRequest {
            limit: Some(10000), // High limit for the global map
            ..Default::default()
        })
    })
    .await
    .map_err(ApiError::TaskJoin)?
    .map_err(ApiError::Internal)?;

    // First pass: collect raw projection state per item. `cluster_raw` is the
    // algorithm's assignment; `cluster` may already reflect a numeric override
    // (legacy). We resolve anchor-based overrides in the second pass once every
    // item's raw cluster is known.
    struct Raw {
        x: f32,
        y: f32,
        z: f32,
        cluster_raw: usize,
        /// Effective cluster as written to disk at last rebuild — may differ
        /// from `cluster_raw` for items moved by propagation/override/anchor.
        cluster_at_write: usize,
        cluster_override: Option<usize>,
        cluster_anchor_id: Option<String>,
        cluster_name: Option<String>,
        cluster_description: Option<String>,
    }
    let mut raw_by_id: std::collections::HashMap<String, Raw> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut item_lookup: std::collections::HashMap<String, ItemRecord> =
        std::collections::HashMap::new();

    for item in items {
        let Some(proj) = item.metadata.get("projection") else {
            continue;
        };
        let (Some(x), Some(y)) = (
            proj.get("x").and_then(|v| v.as_f64()),
            proj.get("y").and_then(|v| v.as_f64()),
        ) else {
            continue;
        };
        let z = proj.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let cluster_at_write = proj
            .get("cluster")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        // `cluster_raw` falls back to `cluster` for items written before the
        // raw/effective split landed.
        let cluster_raw = proj
            .get("cluster_raw")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .or(cluster_at_write);
        let Some(cluster_raw) = cluster_raw else {
            continue;
        };
        let cluster_at_write = cluster_at_write.unwrap_or(cluster_raw);
        let cluster_override = proj
            .get("cluster_override")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let cluster_anchor_id = proj
            .get("cluster_anchor_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let cluster_name = proj
            .get("cluster_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let cluster_description = proj
            .get("cluster_description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        order.push(item.id.clone());
        raw_by_id.insert(
            item.id.clone(),
            Raw {
                x: x as f32,
                y: y as f32,
                z: z as f32,
                cluster_raw,
                cluster_at_write,
                cluster_override,
                cluster_anchor_id,
                cluster_name,
                cluster_description,
            },
        );
        item_lookup.insert(item.id.clone(), item);
    }

    // Build cluster_id → (name, description) from items that are organic
    // members of their cluster (no override of any kind). Anchored / overridden
    // items carry a stale label from a previous bucket, so excluding them
    // prevents "Outliers" from leaking onto the override target.
    let mut cluster_labels: std::collections::HashMap<usize, (Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    for (id, raw) in raw_by_id.iter() {
        if raw.cluster_override.is_some() || raw.cluster_anchor_id.is_some() {
            continue;
        }
        // Skip items moved by propagation — their stored cluster_name is the
        // label of the cluster they were pulled INTO, not the cluster they
        // came FROM. Letting these vote pollutes the noise-bucket label.
        if raw.cluster_at_write != raw.cluster_raw {
            continue;
        }
        let entry = cluster_labels
            .entry(raw.cluster_raw)
            .or_insert((None, None));
        if entry.0.is_none() {
            entry.0 = raw.cluster_name.clone();
        }
        if entry.1.is_none() {
            entry.1 = raw.cluster_description.clone();
        }
        let _ = id;
    }

    let mut out = Vec::with_capacity(order.len());
    for id in order {
        let raw = &raw_by_id[&id];
        let effective_cluster = if let Some(anchor_id) = &raw.cluster_anchor_id {
            raw_by_id
                .get(anchor_id)
                .map(|a| a.cluster_raw)
                .unwrap_or(raw.cluster_raw)
        } else if let Some(c) = raw.cluster_override {
            c
        } else {
            raw.cluster_raw
        };
        let (cluster_name, cluster_description) = cluster_labels
            .get(&effective_cluster)
            .cloned()
            .unwrap_or_else(|| (raw.cluster_name.clone(), raw.cluster_description.clone()));

        let item = &item_lookup[&id];
        let (title, doc_type, tags) = match item.analysis.as_ref() {
            Some(a) => (
                a.get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                a.get("doc_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                a.get("tags").and_then(|v| v.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                }),
            ),
            None => (None, None, None),
        };
        let snippet = {
            let t = item.text.trim();
            if t.is_empty() {
                None
            } else {
                let mut s: String = t.chars().take(160).collect();
                if t.chars().count() > 160 {
                    s.push('…');
                }
                Some(s)
            }
        };
        out.push(crate::projection::MapPoint {
            id: id.clone(),
            x: raw.x,
            y: raw.y,
            z: raw.z,
            cluster: effective_cluster,
            title,
            snippet,
            source_id: Some(item.source_id.clone()),
            path: item.path.clone(),
            doc_type,
            tags: tags.filter(|t| !t.is_empty()),
            cluster_name,
            cluster_description,
            distance: None,
        });
    }

    Ok(out)
}

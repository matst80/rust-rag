use anyhow::{Context, Result};
use linfa::prelude::*;
use linfa_clustering::KMeans;
use linfa_ndarray::Array2;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument, warn};

use crate::config::AnalysisConfig;
use crate::db::{ItemRecord, ListItemsRequest, VectorStore};

pub struct ProjectionWorker {
    store: Arc<dyn VectorStore + Send + Sync>,
    processing: Arc<Mutex<bool>>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MapPoint {
    pub id: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub z: f32,
    pub cluster: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_description: Option<String>,
    /// Populated by `map_get`/`map_nearest` when a `center_id` is supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f32>,
}

struct ClusterResult {
    items_to_update: Vec<ItemRecord>,
    /// Raw algorithmic cluster id per item (parallel to items_to_update).
    assignments: Vec<usize>,
    /// Effective cluster id after applying overrides + anchors. Same length.
    effective: Vec<usize>,
    coords: Vec<(f32, f32, f32)>,
    noise_bucket: Option<usize>,
    /// Persisted user overrides, parallel to items_to_update.
    cluster_overrides: Vec<Option<usize>>,
    cluster_anchor_ids: Vec<Option<String>>,
}

#[derive(Default, Clone, serde::Deserialize)]
struct ClusterLabel {
    name: Option<String>,
    description: Option<String>,
}

impl ProjectionWorker {
    pub fn new(store: Arc<dyn VectorStore + Send + Sync>) -> Self {
        Self {
            store,
            processing: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn is_processing(&self) -> bool {
        *self.processing.lock().await
    }

    #[instrument(skip(self, http_client, analysis), name = "rebuild_projection_map")]
    pub async fn run_rebuild(
        &self,
        http_client: reqwest::Client,
        analysis: Arc<AnalysisConfig>,
    ) -> Result<()> {
        let mut lock = self.processing.lock().await;
        if *lock {
            warn!("projection rebuild already in progress");
            return Ok(());
        }
        *lock = true;
        drop(lock);

        let store = self.store.clone();
        let processing = self.processing.clone();

        tokio::spawn(async move {
            let outcome = Self::do_rebuild(store, http_client, analysis).await;
            if let Err(e) = outcome {
                tracing::error!("projection rebuild failed: {:?}", e);
            }
            *processing.lock().await = false;
        });

        Ok(())
    }

    async fn do_rebuild(
        store: Arc<dyn VectorStore + Send + Sync>,
        http_client: reqwest::Client,
        analysis: Arc<AnalysisConfig>,
    ) -> Result<()> {
        info!("starting global projection rebuild");

        let store_for_compute = store.clone();
        let cluster_result =
            tokio::task::spawn_blocking(move || compute_clusters(store_for_compute))
                .await
                .context("compute_clusters join")??;

        let Some(cluster_result) = cluster_result else {
            info!("no items to project");
            return Ok(());
        };

        // Group items per cluster for labeling
        let mut by_cluster: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, c) in cluster_result.assignments.iter().enumerate() {
            by_cluster.entry(*c).or_default().push(i);
        }

        // Don't send the noise bucket to the LLM; it's a mixed bag by definition.
        let mut by_cluster_for_llm = by_cluster.clone();
        if let Some(nb) = cluster_result.noise_bucket {
            by_cluster_for_llm.remove(&nb);
        }
        info!(
            "labeling {} clusters via LLM (noise bucket skipped: {:?})",
            by_cluster_for_llm.len(),
            cluster_result.noise_bucket
        );
        let mut labels = generate_cluster_labels(
            &http_client,
            &analysis,
            &cluster_result.items_to_update,
            &by_cluster_for_llm,
        )
        .await;
        if let Some(nb) = cluster_result.noise_bucket {
            labels.insert(
                nb,
                ClusterLabel {
                    name: Some("Outliers".to_string()),
                    description: Some(
                        "Points HDBSCAN could not assign to any dense cluster.".to_string(),
                    ),
                },
            );
        }

        // Write metadata back
        let store_for_write = store.clone();
        let cluster_result_arc = Arc::new(cluster_result);
        let labels_arc = Arc::new(labels);
        let cluster_result_for_write = cluster_result_arc.clone();
        let labels_for_write = labels_arc.clone();
        tokio::task::spawn_blocking(move || {
            write_metadata(
                &store_for_write,
                &cluster_result_for_write,
                &labels_for_write,
            )
        })
        .await
        .context("write_metadata join")??;

        info!("projection rebuild complete");
        Ok(())
    }
}

fn compute_clusters(store: Arc<dyn VectorStore + Send + Sync>) -> Result<Option<ClusterResult>> {
    let (items, _) = store.list_items(ListItemsRequest {
        limit: Some(10000),
        ..Default::default()
    })?;

    if items.is_empty() {
        return Ok(None);
    }

    info!("processing {} items", items.len());

    let mut vectors = Vec::new();
    let mut items_to_update = Vec::new();
    let mut overrides: Vec<Option<usize>> = Vec::new();
    let mut anchor_ids: Vec<Option<String>> = Vec::new();
    let mut rng = rand::thread_rng();
    use rand::Rng;

    for item in items {
        let chunks = store.get_item_chunks(&item.id)?;
        if chunks.is_empty() {
            continue;
        }

        let mut sum_vec: Option<Vec<f32>> = None;
        let mut count = 0;

        for chunk in chunks {
            let emb = chunk.embedding;
            if let Some(ref mut sum) = sum_vec {
                for (s, v) in sum.iter_mut().zip(emb.iter()) {
                    *s += v;
                }
            } else {
                sum_vec = Some(emb);
            }
            count += 1;
        }

        if let Some(mut avg) = sum_vec {
            if count > 1 {
                for v in avg.iter_mut() {
                    *v /= count as f32;
                }
            }
            // L2-normalise: turns Euclidean distance into angular distance,
            // matching how embeddings are usually compared.
            let norm = avg.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 1e-8 {
                for v in avg.iter_mut() {
                    *v /= norm;
                }
            }
            for v in avg.iter_mut() {
                *v += (rng.r#gen::<f32>() - 0.5) * 1e-5;
            }
            let proj = item.metadata.get("projection");
            let cluster_override = proj
                .and_then(|p| p.get("cluster_override"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let anchor_id = proj
                .and_then(|p| p.get("cluster_anchor_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            overrides.push(cluster_override);
            anchor_ids.push(anchor_id);
            vectors.push(avg);
            items_to_update.push(item);
        }
    }

    if vectors.is_empty() {
        return Ok(None);
    }

    let n_samples = vectors.len();
    let n_features = vectors[0].len();
    let algo = std::env::var("RAG_PROJECTION_ALGO")
        .unwrap_or_else(|_| "kmeans".into())
        .to_lowercase();

    let raw_assignments: Vec<i64> = if algo == "hdbscan" {
        info!("running HDBSCAN (parallel)");
        let min_cluster_size = std::env::var("RAG_HDBSCAN_MIN_CLUSTER_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or_else(|| ((n_samples as f32).sqrt().round() as usize).clamp(5, 50));
        let min_samples = std::env::var("RAG_HDBSCAN_MIN_SAMPLES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(min_cluster_size.saturating_sub(1).max(1));
        let data: Vec<Vec<f32>> = vectors.clone();
        let hp = hdbscan::HdbscanHyperParams::builder()
            .min_cluster_size(min_cluster_size)
            .min_samples(min_samples)
            .dist_metric(hdbscan::DistanceMetric::Euclidean)
            .build();
        let clusterer = hdbscan::Hdbscan::new(&data, hp);
        let labels = clusterer
            .cluster_par()
            .map_err(|e| anyhow::anyhow!("HDBSCAN: {e}"))?;
        labels.into_iter().map(|v| v as i64).collect()
    } else {
        info!("running KMeans (n_runs=10)");
        let flattened: Vec<f32> = vectors.iter().flatten().copied().collect();
        let data_f64: Vec<f64> = flattened.iter().map(|v| *v as f64).collect();
        let nd_data = Array2::from_shape_vec((n_samples, n_features), data_f64)
            .context("building data matrix")?;
        let k = (n_samples as f32).sqrt().round() as usize;
        let k = k.clamp(2, 50);
        let dataset = Dataset::from(nd_data);
        let model = KMeans::params(k)
            .max_n_iterations(100)
            .n_runs(10)
            .fit(&dataset)
            .map_err(|e| anyhow::anyhow!("KMeans fitting: {:?}", e))?;
        model.predict(&dataset).iter().map(|&c| c as i64).collect()
    };

    // Remap to dense usize ids; HDBSCAN noise (-1) goes into its own bucket
    // labelled at the end, KMeans ids stay stable.
    let max_cluster = raw_assignments.iter().copied().max().unwrap_or(0).max(0) as usize;
    let has_noise = raw_assignments.iter().any(|&c| c < 0);
    let noise_bucket = if has_noise {
        Some(max_cluster + 1)
    } else {
        None
    };
    let assignments: Vec<usize> = raw_assignments
        .iter()
        .map(|&c| {
            if c < 0 {
                noise_bucket.expect("noise_bucket set when noise present")
            } else {
                c as usize
            }
        })
        .collect();

    // Effective assignments: anchors resolve to anchor's raw cluster, then
    // numeric overrides apply, otherwise raw assignment wins.
    let id_to_idx: std::collections::HashMap<String, usize> = items_to_update
        .iter()
        .enumerate()
        .map(|(i, item)| (item.id.clone(), i))
        .collect();
    let mut effective: Vec<usize> = (0..assignments.len())
        .map(|i| {
            if let Some(anchor) = anchor_ids[i].as_deref() {
                if let Some(&j) = id_to_idx.get(anchor) {
                    return assignments[j];
                }
            }
            if let Some(c) = overrides[i] {
                return c;
            }
            assignments[i]
        })
        .collect();

    // Graph-edge label propagation: pull noise-bucket items into the cluster
    // their confirmed manual neighbours belong to. Skip items that already
    // carry a user override/anchor — those are explicit human decisions.
    let propagate = std::env::var("RAG_PROJECTION_PROPAGATE_NOISE")
        .map(|v| !matches!(v.as_str(), "0" | "false" | "no"))
        .unwrap_or(true);
    let rounds = std::env::var("RAG_PROJECTION_PROPAGATE_ROUNDS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2);
    let min_score = std::env::var("RAG_PROJECTION_PROPAGATE_MIN_SCORE")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.5);

    if propagate {
        if let Some(noise) = noise_bucket {
            let mut moved_total = 0usize;
            for round in 0..rounds {
                let mut moved = 0usize;
                let snapshot = effective.clone();
                for i in 0..effective.len() {
                    if effective[i] != noise {
                        continue;
                    }
                    if overrides[i].is_some() || anchor_ids[i].is_some() {
                        continue;
                    }
                    let edges = match store.list_graph_edges(
                        Some(&items_to_update[i].id),
                        Some(crate::db::GraphEdgeType::Manual),
                        None,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!(
                                "list_graph_edges for {} failed: {e:?}",
                                items_to_update[i].id
                            );
                            continue;
                        }
                    };
                    let mut votes: std::collections::HashMap<usize, f32> =
                        std::collections::HashMap::new();
                    for edge in edges {
                        if edge.weight <= 0.0 {
                            continue;
                        }
                        let other = if edge.from_item_id == items_to_update[i].id {
                            &edge.to_item_id
                        } else {
                            &edge.from_item_id
                        };
                        if let Some(&j) = id_to_idx.get(other) {
                            let nc = snapshot[j];
                            if nc == noise {
                                continue;
                            }
                            *votes.entry(nc).or_insert(0.0) += edge.weight;
                        }
                    }
                    if let Some((&winner, &score)) =
                        votes.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                    {
                        if score >= min_score {
                            effective[i] = winner;
                            moved += 1;
                        }
                    }
                }
                moved_total += moved;
                info!(
                    "propagation round {}: moved {} noise items",
                    round + 1,
                    moved
                );
                if moved == 0 {
                    break;
                }
            }
            if moved_total > 0 {
                info!("graph propagation rescued {moved_total} noise items into dense clusters");
            }
        }
    }

    info!("running PCA reduction to 3D");
    let flattened: Vec<f32> = vectors.into_iter().flatten().collect();
    let data_f64: Vec<f64> = flattened.iter().map(|v| *v as f64).collect();
    let nd_data = Array2::from_shape_vec((n_samples, n_features), data_f64)
        .context("building data matrix for PCA")?;
    let dataset = Dataset::from(nd_data);
    use linfa_reduction::Pca;
    let n_components = 3usize.min(n_features);
    let pca = Pca::params(n_components)
        .fit(&dataset)
        .map_err(|e| anyhow::anyhow!("PCA fitting: {:?}", e))?;
    let coords_matrix = pca.predict(&dataset);

    let coords: Vec<(f32, f32, f32)> = (0..items_to_update.len())
        .map(|i| {
            let x = coords_matrix[[i, 0]] as f32;
            let y = coords_matrix[[i, 1]] as f32;
            let z = if n_components > 2 {
                coords_matrix[[i, 2]] as f32
            } else {
                0.0
            };
            (x, y, z)
        })
        .collect();

    Ok(Some(ClusterResult {
        items_to_update,
        assignments,
        effective,
        coords,
        noise_bucket,
        cluster_overrides: overrides,
        cluster_anchor_ids: anchor_ids,
    }))
}

async fn generate_cluster_labels(
    http_client: &reqwest::Client,
    analysis: &AnalysisConfig,
    items: &[ItemRecord],
    by_cluster: &HashMap<usize, Vec<usize>>,
) -> HashMap<usize, ClusterLabel> {
    let mut out = HashMap::new();

    let base_url = match analysis.base_url.as_deref() {
        Some(u) => u,
        None => {
            warn!("analysis base_url missing — skipping cluster labeling");
            return out;
        }
    };
    let model = match analysis.model.as_deref() {
        Some(m) => m,
        None => {
            warn!("analysis model missing — skipping cluster labeling");
            return out;
        }
    };

    for (cluster_id, indices) in by_cluster {
        let samples: Vec<String> = indices
            .iter()
            .take(8)
            .map(|&i| {
                let item = &items[i];
                let title = item
                    .analysis
                    .as_ref()
                    .and_then(|a| a.get("title"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| item.id.clone());
                let snippet: String = item.text.chars().take(200).collect();
                format!("- {title}\n  {snippet}")
            })
            .collect();

        let user_prompt = format!(
            "Below are {n} representative entries from one cluster of a knowledge base. \
             Produce a short, specific cluster label.\n\n{samples}\n\n\
             Return JSON: {{\"name\": \"...\", \"description\": \"...\"}}. \
             `name` ≤ 4 words, title case, no quotes. `description` ≤ 16 words.",
            n = samples.len(),
            samples = samples.join("\n")
        );

        let req = crate::api::analysis::ChatCompletionRequest {
            base_url,
            api_key: analysis.api_key.as_deref(),
            model,
            timeout_secs: analysis.timeout_secs,
            system_prompt: "You label topical clusters of knowledge-base entries. Be concise and concrete.",
            user_prompt: &user_prompt,
            max_tokens: 200,
            temperature: 0.2,
            response_format_json: true,
        };

        // Compute a deterministic fallback up-front: first sample's title.
        // Used when the LLM returns empty or unparseable JSON.
        let fallback_name = indices
            .first()
            .and_then(|&i| {
                items[i]
                    .analysis
                    .as_ref()
                    .and_then(|a| a.get("title"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("Cluster {cluster_id}"));

        match crate::api::analysis::chat_completion_text(http_client, req).await {
            Ok(raw) => {
                let parsed = parse_label(&raw);
                if parsed.name.is_some() {
                    out.insert(*cluster_id, parsed);
                } else {
                    warn!(
                        "cluster {} label parse failed, falling back to '{}' — raw: {:?}",
                        cluster_id, fallback_name, raw
                    );
                    out.insert(
                        *cluster_id,
                        ClusterLabel {
                            name: Some(fallback_name),
                            description: parsed.description,
                        },
                    );
                }
            }
            Err(e) => {
                warn!(
                    "cluster {} label LLM error: {:?} — falling back to '{}'",
                    cluster_id, e, fallback_name
                );
                out.insert(
                    *cluster_id,
                    ClusterLabel {
                        name: Some(fallback_name),
                        description: None,
                    },
                );
            }
        }
    }

    out
}

fn parse_label(raw: &str) -> ClusterLabel {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_end_matches("```").trim())
        .unwrap_or(trimmed);

    // Try strict JSON first (object slice between outer braces).
    let start = stripped.find('{');
    let end = stripped.rfind('}');
    if let (Some(s), Some(e)) = (start, end) {
        if e > s {
            if let Ok(parsed) = serde_json::from_str::<ClusterLabel>(&stripped[s..=e]) {
                if parsed.name.is_some() {
                    return parsed;
                }
            }
        }
    }
    // Recover truncated JSON: opening brace but no closing.
    if let Some(s) = start {
        if end.map(|e| e <= s).unwrap_or(true) {
            let patched = format!("{}}}", &stripped[s..]);
            if let Ok(parsed) = serde_json::from_str::<ClusterLabel>(&patched) {
                if parsed.name.is_some() {
                    return parsed;
                }
            }
        }
    }
    // Bare-string fallback: model ignored JSON instruction. Strip quotes and
    // trailing punctuation, take the first ≤ 60 chars as a label name.
    let bare = stripped.trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());
    if !bare.is_empty() && bare.len() < 200 && !bare.contains('\n') {
        let name: String = bare.chars().take(60).collect();
        return ClusterLabel {
            name: Some(name),
            description: None,
        };
    }
    ClusterLabel::default()
}

fn write_metadata(
    store: &Arc<dyn VectorStore + Send + Sync>,
    result: &ClusterResult,
    labels: &HashMap<usize, ClusterLabel>,
) -> Result<()> {
    info!(
        "writing projection metadata for {} items",
        result.items_to_update.len()
    );
    for (i, item) in result.items_to_update.iter().enumerate() {
        let (x, y, z) = result.coords[i];
        let cluster_id = result.assignments[i];

        let mut metadata = item.metadata.clone();
        let obj = match metadata.as_object_mut() {
            Some(o) => o,
            None => {
                metadata = serde_json::json!({});
                metadata.as_object_mut().unwrap()
            }
        };
        let raw_cluster = result.assignments[i];
        let effective_cluster = result.effective[i];
        let mut map_data = serde_json::Map::new();
        map_data.insert("x".to_string(), serde_json::json!(x));
        map_data.insert("y".to_string(), serde_json::json!(y));
        map_data.insert("z".to_string(), serde_json::json!(z));
        // `cluster` is the EFFECTIVE id (post-override/anchor). `cluster_raw`
        // is the algorithm's untouched assignment — render-time label
        // resolution uses raw labels of organic members.
        map_data.insert("cluster".to_string(), serde_json::json!(effective_cluster));
        map_data.insert("cluster_raw".to_string(), serde_json::json!(raw_cluster));
        if let Some(v) = result.cluster_overrides[i] {
            map_data.insert("cluster_override".to_string(), serde_json::json!(v));
        }
        if let Some(a) = &result.cluster_anchor_ids[i] {
            map_data.insert("cluster_anchor_id".to_string(), serde_json::json!(a));
        }
        // Store the label for the EFFECTIVE cluster so legacy readers that
        // don't do render-time resolution still see something sensible. The
        // new build_map_points overrides this at render time anyway.
        if let Some(label) = labels.get(&effective_cluster) {
            if let Some(name) = &label.name {
                map_data.insert("cluster_name".to_string(), serde_json::json!(name));
            }
            if let Some(desc) = &label.description {
                map_data.insert("cluster_description".to_string(), serde_json::json!(desc));
            }
        }
        obj.insert(
            "projection".to_string(),
            serde_json::Value::Object(map_data),
        );

        if let Err(e) = store.update_item_metadata(&item.id, metadata) {
            warn!("failed to update metadata for {}: {:?}", item.id, e);
        }
    }
    Ok(())
}

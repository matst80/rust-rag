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
    assignments: Vec<usize>,
    coords: Vec<(f32, f32, f32)>,
    noise_bucket: Option<usize>,
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
        let cluster_result = tokio::task::spawn_blocking(move || compute_clusters(store_for_compute))
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
            write_metadata(&store_for_write, &cluster_result_for_write, &labels_for_write)
        })
        .await
        .context("write_metadata join")??;

        info!("projection rebuild complete");
        Ok(())
    }
}

fn compute_clusters(
    store: Arc<dyn VectorStore + Send + Sync>,
) -> Result<Option<ClusterResult>> {
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
            let cluster_override = item
                .metadata
                .get("projection")
                .and_then(|p| p.get("cluster_override"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            overrides.push(cluster_override);
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
    let noise_bucket = if has_noise { Some(max_cluster + 1) } else { None };
    let mut assignments: Vec<usize> = raw_assignments
        .iter()
        .map(|&c| {
            if c < 0 {
                noise_bucket.expect("noise_bucket set when noise present")
            } else {
                c as usize
            }
        })
        .collect();

    // Apply user overrides last so manual reassignments survive every rebuild.
    for (i, ov) in overrides.iter().enumerate() {
        if let Some(c) = ov {
            assignments[i] = *c;
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
        coords,
        noise_bucket,
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

        match crate::api::analysis::chat_completion_text(http_client, req).await {
            Ok(raw) => {
                let parsed = parse_label(&raw);
                if parsed.name.is_some() {
                    out.insert(*cluster_id, parsed);
                }
            }
            Err(e) => warn!("cluster {} label LLM error: {:?}", cluster_id, e),
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
    let start = stripped.find('{');
    let end = stripped.rfind('}');
    let candidate = match (start, end) {
        (Some(s), Some(e)) if e > s => &stripped[s..=e],
        _ => stripped,
    };
    serde_json::from_str::<ClusterLabel>(candidate).unwrap_or_default()
}

fn write_metadata(
    store: &Arc<dyn VectorStore + Send + Sync>,
    result: &ClusterResult,
    labels: &HashMap<usize, ClusterLabel>,
) -> Result<()> {
    info!("writing projection metadata for {} items", result.items_to_update.len());
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
        let prior_override = obj
            .get("projection")
            .and_then(|p| p.get("cluster_override"))
            .cloned();
        let mut map_data = serde_json::Map::new();
        map_data.insert("x".to_string(), serde_json::json!(x));
        map_data.insert("y".to_string(), serde_json::json!(y));
        map_data.insert("z".to_string(), serde_json::json!(z));
        map_data.insert("cluster".to_string(), serde_json::json!(cluster_id));
        if let Some(v) = prior_override {
            map_data.insert("cluster_override".to_string(), v);
        }
        if let Some(label) = labels.get(&cluster_id) {
            if let Some(name) = &label.name {
                map_data.insert("cluster_name".to_string(), serde_json::json!(name));
            }
            if let Some(desc) = &label.description {
                map_data.insert("cluster_description".to_string(), serde_json::json!(desc));
            }
        }
        obj.insert("projection".to_string(), serde_json::Value::Object(map_data));

        if let Err(e) = store.update_item_metadata(&item.id, metadata) {
            warn!("failed to update metadata for {}: {:?}", item.id, e);
        }
    }
    Ok(())
}

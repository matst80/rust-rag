pub mod acp;
pub mod analysis;
pub mod attachments;
mod auth;
pub mod auth_guard;
mod chunking;
pub mod cms;
pub mod dream;
mod dreaming;
pub mod error;
pub mod graph;
pub mod health;
mod ingest_url;
mod integrations;
pub mod items;
pub mod map;
pub mod messages;
mod multimodal;
mod ontology;
mod openai;
mod presence;
mod push;
mod query;
pub mod router;
pub mod schemas;
pub mod state;
pub mod store_search;
mod tombstones;
pub mod whisper;

pub use analysis::{
    AnalyzeEntryParams, ChatCompletionRequest, StoreAnalysis, chat_completion_text, run_analysis,
};
pub use auth::SessionSubject;
pub use dreaming::{process_dreaming_round, run_dreaming_worker};
pub use presence::{PresenceEntry, PresenceTracker};
pub use tombstones::{Tombstone, TombstoneTracker};

pub use error::ApiError;
pub(crate) use error::{current_timestamp_millis, metadata_schema};
pub use map::build_map_points;
pub use router::router;
pub use state::{AppState, EmbedderHandle};
pub use store_search::{
    ChunkConfig, SearchRequest, SearchResponse, SearchResultPayload, StoreRequest, StoreResponse,
};
pub(crate) use store_search::{append_to_entry_core, search_core, store_entry_core};

// Re-exports for tests and crate-internal call sites that used flat paths.
pub(crate) use error::{
    api_validation_error, map_graph_error, resolve_store_id, validate_graph_depth,
    validate_graph_limit, validate_metadata, validate_non_empty, validate_source_id,
};
pub(crate) use graph::{
    CreateManualEdgeRequest, GraphEdgePayload, GraphEdgesResponse, GraphNeighborhoodQuery,
    GraphNeighborhoodResponse, GraphRebuildResponse, GraphStatusResponse, ListGraphEdgesQuery,
};
pub(crate) use health::HealthResponse;
pub(crate) use items::{
    AdminItemPayload, AdminItemsResponse, CategoriesResponse, EntryNeighbor, ListItemsQuery,
    UpdateItemRequest,
};
pub(crate) use messages::{ActiveUserPayload, ClearChannelResponse, MessagePayload};
pub(crate) use store_search::{DeleteResponse, invalidate_cms_nodes};

#[cfg(test)]
pub(crate) use state::{NoopMessages, NoopUserMemory};

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use axum_test::TestServer;
    use serde_json::json;
    use std::{
        collections::{BTreeMap, HashMap, HashSet, VecDeque},
        sync::Mutex,
    };

    struct MockEmbedder {
        embedding: Vec<f32>,
        seen_inputs: Mutex<Vec<String>>,
    }

    impl MockEmbedder {
        fn new(embedding: Vec<f32>) -> Self {
            Self {
                embedding,
                seen_inputs: Mutex::new(Vec::new()),
            }
        }
    }

    impl EmbeddingService for MockEmbedder {
        fn embed(&self, text: &str) -> Result<Vec<f32>> {
            self.seen_inputs
                .lock()
                .expect("embedder mutex poisoned")
                .push(text.to_owned());
            Ok(self.embedding.clone())
        }

        fn count_tokens(&self, text: &str) -> Result<usize> {
            Ok(text.split_whitespace().count())
        }
    }

    struct MockStore {
        stored: Mutex<Vec<(ItemRecord, Vec<f32>)>>,
        messages: Mutex<Vec<MessageRecord>>,
        search_results: Mutex<Vec<SearchHit>>,
        search_source_ids: Mutex<Vec<Option<String>>>,
        graph_enabled: bool,
        graph_edges: Mutex<Vec<GraphEdgeRecord>>,
        graph_rebuilds: Mutex<usize>,
        mcp_tokens: Mutex<Vec<crate::db::McpTokenRecord>>,
        mcp_token_hashes: Mutex<HashMap<String, String>>,
        device_auths: Mutex<Vec<crate::db::DeviceAuthRecord>>,
        auth_codes: Mutex<Vec<crate::db::OAuthAuthCodeRecord>>,
    }

    impl Default for MockStore {
        fn default() -> Self {
            Self {
                stored: Mutex::new(Vec::new()),
                messages: Mutex::new(Vec::new()),
                search_results: Mutex::new(Vec::new()),
                search_source_ids: Mutex::new(Vec::new()),
                graph_enabled: false,
                graph_edges: Mutex::new(Vec::new()),
                graph_rebuilds: Mutex::new(0),
                mcp_tokens: Mutex::new(Vec::new()),
                mcp_token_hashes: Mutex::new(HashMap::new()),
                device_auths: Mutex::new(Vec::new()),
                auth_codes: Mutex::new(Vec::new()),
            }
        }
    }

    impl MockStore {
        fn with_results(results: Vec<SearchHit>) -> Self {
            Self {
                search_results: Mutex::new(results),
                ..Self::default()
            }
        }

        fn seed(items: Vec<ItemRecord>) -> Self {
            Self {
                stored: Mutex::new(items.into_iter().map(|item| (item, Vec::new())).collect()),
                ..Self::default()
            }
        }

        fn seed_messages(messages: Vec<MessageRecord>) -> Self {
            Self {
                messages: Mutex::new(messages),
                ..Self::default()
            }
        }

        fn seed_graph(items: Vec<ItemRecord>, edges: Vec<GraphEdgeRecord>) -> Self {
            Self {
                stored: Mutex::new(items.into_iter().map(|item| (item, Vec::new())).collect()),
                graph_enabled: true,
                graph_edges: Mutex::new(edges),
                ..Self::default()
            }
        }
    }

    impl VectorStore for MockStore {
        fn upsert_item(&self, item: ItemRecord, embedding: &[f32]) -> Result<()> {
            let mut stored = self.stored.lock().expect("store mutex poisoned");
            if let Some(existing) = stored
                .iter_mut()
                .find(|(existing, _)| existing.id == item.id)
            {
                *existing = (item, embedding.to_vec());
            } else {
                stored.push((item, embedding.to_vec()));
            }
            Ok(())
        }

        fn search(
            &self,
            _query_embedding: &[f32],
            _top_k: usize,
            source_id: Option<&str>,
            _type_name: Option<&str>,
        ) -> Result<Vec<SearchHit>> {
            self.search_source_ids
                .lock()
                .expect("store mutex poisoned")
                .push(source_id.map(str::to_owned));
            Ok(self
                .search_results
                .lock()
                .expect("store mutex poisoned")
                .clone())
        }

        fn search_hybrid(
            &self,
            _query_text: &str,
            query_embedding: &[f32],
            _query_sparse: &[(u32, f32)],
            top_k: usize,
            source_id: Option<&str>,
            _type_name: Option<&str>,
        ) -> Result<Vec<SearchHit>> {
            self.search(query_embedding, top_k, source_id, None)
        }

        fn distances_for_ids(
            &self,
            _query_embedding: &[f32],
            ids: &[String],
        ) -> Result<Vec<SearchHit>> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            let results = self.search_results.lock().expect("store mutex poisoned");
            let mut hits = Vec::new();
            for id in ids {
                if let Some(hit) = results.iter().find(|h| &h.id == id) {
                    hits.push(hit.clone());
                    continue;
                }
                if let Some((item, _)) = stored.iter().find(|(item, _)| &item.id == id) {
                    hits.push(SearchHit {
                        id: item.id.clone(),
                        text: item.text.clone(),
                        metadata: item.metadata.clone(),
                        source_id: item.source_id.clone(),
                        created_at: item.created_at,
                        distance: 0.0,
                        section_path: Vec::new(),
                        retrievers: Vec::new(),
                        chunk_text: None,
                        path: item.path.clone(),
                        type_name: item.type_name.clone(),
                        tags: item
                            .metadata
                            .get("tags")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .collect()
                            })
                            .unwrap_or_default(),
                        analysis: item.analysis.clone(),
                        updated_at: item.updated_at,
                    });
                }
            }
            Ok(hits)
        }

        fn list_categories(&self) -> Result<Vec<CategorySummary>> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            let mut counts = BTreeMap::<String, i64>::new();
            for (item, _) in stored.iter() {
                *counts.entry(item.source_id.clone()).or_default() += 1;
            }
            Ok(counts
                .into_iter()
                .map(|(source_id, item_count)| CategorySummary {
                    source_id,
                    item_count,
                })
                .collect())
        }

        fn list_items(&self, request: ListItemsRequest) -> Result<(Vec<ItemRecord>, i64)> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            let mut items = stored
                .iter()
                .filter(|(item, _)| {
                    if let Some(source) = &request.source_id {
                        if &item.source_id != source {
                            return false;
                        }
                    }
                    if let Some(min) = request.min_created_at {
                        if item.created_at < min {
                            return false;
                        }
                    }
                    if let Some(max) = request.max_created_at {
                        if item.created_at > max {
                            return false;
                        }
                    }
                    for (key, val) in &request.metadata_filter {
                        if let Some(meta_val) = item.metadata.get(key) {
                            if meta_val.as_str() != Some(val) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    true
                })
                .map(|(item, _)| item.clone())
                .collect::<Vec<_>>();

            let total_count = items.len() as i64;

            items.sort_by(|a, b| {
                let ordering = b
                    .created_at
                    .cmp(&a.created_at)
                    .then_with(|| a.id.cmp(&b.id));
                match request.sort_order {
                    SortOrder::Asc => ordering.reverse(),
                    SortOrder::Desc => ordering,
                }
            });

            let offset = request.offset.unwrap_or(0);
            let limit = request.limit.unwrap_or(100);
            let paged_items = items.into_iter().skip(offset).take(limit).collect();

            Ok((paged_items, total_count))
        }

        fn get_item(&self, id: &str) -> Result<Option<ItemRecord>> {
            let stored = self.stored.lock().expect("store mutex poisoned");
            Ok(stored
                .iter()
                .find(|(item, _)| item.id == id)
                .map(|(item, _)| item.clone()))
        }

        fn delete_item(&self, id: &str) -> Result<bool> {
            let mut stored = self.stored.lock().expect("store mutex poisoned");
            let before = stored.len();
            stored.retain(|(item, _)| item.id != id);
            Ok(stored.len() != before)
        }

        fn graph_status(&self) -> Result<GraphStatus> {
            let item_count = self.stored.lock().expect("store mutex poisoned").len() as i64;
            let edges = self.graph_edges.lock().expect("store mutex poisoned");
            let similarity_edge_count = edges
                .iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Similarity)
                .count() as i64;
            let manual_edge_count = edges
                .iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Manual)
                .count() as i64;

            Ok(GraphStatus {
                enabled: self.graph_enabled,
                build_on_startup: false,
                similarity_top_k: 5,
                similarity_max_distance: 0.75,
                cross_source: false,
                item_count,
                edge_count: edges.len() as i64,
                similarity_edge_count,
                manual_edge_count,
            })
        }

        fn graph_neighborhood(
            &self,
            center_id: &str,
            depth: usize,
            limit: usize,
            edge_type: Option<GraphEdgeType>,
        ) -> Result<GraphNeighborhood> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let items = self
                .stored
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .map(|(item, _)| item.clone())
                .collect::<Vec<_>>();
            let item_index = items
                .iter()
                .map(|item| (item.id.clone(), item.clone()))
                .collect::<HashMap<_, _>>();
            if !item_index.contains_key(center_id) {
                anyhow::bail!("item {center_id} not found");
            }

            let edges = self.list_graph_edges(None, edge_type, None)?;
            let mut visited = HashSet::from([center_id.to_owned()]);
            let mut order = vec![center_id.to_owned()];
            let mut queue = VecDeque::from([(center_id.to_owned(), 0usize)]);
            let mut neighborhood_edges = HashMap::new();

            while let Some((current_id, current_depth)) = queue.pop_front() {
                if current_depth >= depth {
                    continue;
                }

                for edge in edges
                    .iter()
                    .filter(|edge| edge.from_item_id == current_id || edge.to_item_id == current_id)
                {
                    neighborhood_edges.insert(edge.id.clone(), edge.clone());
                    for next in [&edge.from_item_id, &edge.to_item_id] {
                        if visited.len() >= limit || visited.contains(next) {
                            continue;
                        }
                        visited.insert(next.clone());
                        order.push(next.clone());
                        queue.push_back((next.clone(), current_depth + 1));
                    }
                }
            }

            let nodes = order
                .into_iter()
                .filter_map(|id| item_index.get(&id).cloned())
                .collect::<Vec<_>>();
            let mut edges = neighborhood_edges
                .into_values()
                .filter(|edge| {
                    visited.contains(&edge.from_item_id) && visited.contains(&edge.to_item_id)
                })
                .collect::<Vec<_>>();
            edges.sort_by(|a, b| a.id.cmp(&b.id));

            Ok(GraphNeighborhood {
                center_id: center_id.to_owned(),
                nodes,
                edges,
                pairwise_distances: vec![],
            })
        }

        fn get_graph_edge(&self, id: &str) -> Result<Option<GraphEdgeRecord>> {
            let edges = self.graph_edges.lock().expect("store mutex poisoned");
            Ok(edges.iter().find(|e| e.id == id).cloned())
        }

        fn list_graph_edges(
            &self,
            item_id: Option<&str>,
            edge_type: Option<GraphEdgeType>,
            _status: Option<&str>,
        ) -> Result<Vec<GraphEdgeRecord>> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let edges = self.graph_edges.lock().expect("store mutex poisoned");
            Ok(edges
                .iter()
                .filter(|edge| {
                    item_id.is_none_or(|id| edge.from_item_id == id || edge.to_item_id == id)
                })
                .filter(|edge| edge_type.is_none_or(|kind| edge.edge_type == kind))
                .cloned()
                .collect())
        }

        fn rebuild_similarity_graph(&self) -> Result<usize> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            *self.graph_rebuilds.lock().expect("store mutex poisoned") += 1;
            Ok(self
                .graph_edges
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Similarity)
                .count())
        }

        fn list_duplicate_edges(&self) -> Result<Vec<DuplicateEdgeGroup>> {
            Ok(Vec::new())
        }

        fn add_manual_edge(&self, input: ManualEdgeInput) -> Result<GraphEdgeRecord> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let items = self.stored.lock().expect("store mutex poisoned");
            if !items.iter().any(|(item, _)| item.id == input.from_item_id) {
                anyhow::bail!("item {} not found", input.from_item_id);
            }
            if !items.iter().any(|(item, _)| item.id == input.to_item_id) {
                anyhow::bail!("item {} not found", input.to_item_id);
            }
            drop(items);

            let mut edges = self.graph_edges.lock().expect("store mutex poisoned");
            let timestamp = edges.len() as i64 + 1;
            let edge = GraphEdgeRecord {
                id: format!("manual-{}", edges.len() + 1),
                from_item_id: input.from_item_id,
                to_item_id: input.to_item_id,
                edge_type: GraphEdgeType::Manual,
                relation: input.relation.map(|r| r.into_owned()),
                sort_order: input
                    .sort_order
                    .unwrap_or_else(|| crate::db::format_edge_sort_order(timestamp)),
                weight: input.weight,
                directed: input.directed,
                metadata: input.metadata,
                created_at: timestamp,
                updated_at: timestamp,
            };
            edges.push(edge.clone());
            Ok(edge)
        }

        fn update_graph_edge(
            &self,
            id: &str,
            relation: Option<String>,
            metadata: Value,
            sort_order: Option<String>,
        ) -> Result<GraphEdgeRecord> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let mut edges = self.graph_edges.lock().expect("store mutex poisoned");
            let edge = edges
                .iter_mut()
                .find(|edge| edge.id == id)
                .ok_or_else(|| anyhow!("edge {} not found", id))?;

            edge.relation = relation;
            if let Some(sort_order) = sort_order {
                edge.sort_order = crate::db::normalize_edge_sort_order(&sort_order)?;
            }
            edge.metadata = metadata;
            edge.updated_at += 1;
            Ok(edge.clone())
        }

        fn delete_graph_edge(&self, id: &str) -> Result<bool> {
            if !self.graph_enabled {
                anyhow::bail!("graph support is disabled");
            }

            let mut edges = self.graph_edges.lock().expect("store mutex poisoned");
            if edges
                .iter()
                .any(|edge| edge.id == id && edge.edge_type == GraphEdgeType::Similarity)
            {
                anyhow::bail!("similarity edges must be rebuilt, not deleted manually");
            }
            let before = edges.len();
            edges.retain(|edge| edge.id != id);
            Ok(edges.len() != before)
        }

        fn get_items_pending_ontology(&self, _limit: usize) -> Result<Vec<ItemRecord>> {
            Ok(Vec::new())
        }

        fn mark_ontology_status(&self, _id: &str, _status: &str) -> Result<()> {
            Ok(())
        }
    }

    impl MessageStore for MockStore {
        fn get_message(&self, id: &str) -> Result<Option<MessageRecord>> {
            let messages = self.messages.lock().expect("store mutex poisoned");
            Ok(messages.iter().find(|message| message.id == id).cloned())
        }

        fn send_message(&self, message: NewMessage) -> Result<MessageRecord> {
            let record = MessageRecord {
                id: message.id,
                channel: message.channel,
                sender: message.sender,
                sender_kind: message.sender_kind,
                text: message.text,
                kind: message.kind,
                metadata: message.metadata,
                created_at: message.created_at,
                updated_at: message.created_at,
            };
            self.messages
                .lock()
                .expect("store mutex poisoned")
                .push(record.clone());
            Ok(record)
        }

        fn update_message(
            &self,
            id: &str,
            update: MessageUpdate,
            now: i64,
        ) -> Result<Option<MessageRecord>> {
            let mut messages = self.messages.lock().expect("store mutex poisoned");
            let Some(record) = messages.iter_mut().find(|message| message.id == id) else {
                return Ok(None);
            };
            if let Some(text) = update.text {
                if update.append_text {
                    record.text.push_str(&text);
                } else {
                    record.text = text;
                }
            }
            if let Some(metadata) = update.metadata {
                record.metadata = metadata;
            }
            record.updated_at = now;
            Ok(Some(record.clone()))
        }

        fn delete_message(&self, id: &str) -> Result<Option<MessageRecord>> {
            let mut messages = self.messages.lock().expect("store mutex poisoned");
            let Some(index) = messages.iter().position(|message| message.id == id) else {
                return Ok(None);
            };
            Ok(Some(messages.remove(index)))
        }

        fn find_permission_request(&self, request_id: &str) -> Result<Vec<MessageRecord>> {
            let messages = self.messages.lock().expect("store mutex poisoned");
            Ok(messages
                .iter()
                .filter(|message| {
                    message.kind == "permission_request"
                        && message.metadata.get("request_id").and_then(Value::as_str)
                            == Some(request_id)
                })
                .cloned()
                .collect())
        }

        fn list_channel_messages(&self, channel: &str) -> Result<Vec<MessageRecord>> {
            let messages = self.messages.lock().expect("store mutex poisoned");
            Ok(messages
                .iter()
                .filter(|message| message.channel == channel)
                .cloned()
                .collect())
        }

        fn clear_channel(&self, channel: &str) -> Result<Vec<MessageRecord>> {
            let mut messages = self.messages.lock().expect("store mutex poisoned");
            let mut removed = Vec::new();
            let mut kept = Vec::with_capacity(messages.len());
            for message in messages.drain(..) {
                if message.channel == channel {
                    removed.push(message);
                } else {
                    kept.push(message);
                }
            }
            *messages = kept;
            Ok(removed)
        }

        fn list_messages(&self, query: MessageQuery) -> Result<(Vec<MessageRecord>, i64)> {
            let messages = self.messages.lock().expect("store mutex poisoned");
            let mut filtered = messages
                .iter()
                .filter(|message| {
                    query
                        .channel
                        .as_ref()
                        .is_none_or(|channel| &message.channel == channel)
                })
                .filter(|message| {
                    query
                        .sender
                        .as_ref()
                        .is_none_or(|sender| &message.sender == sender)
                })
                .filter(|message| query.kind.as_ref().is_none_or(|kind| &message.kind == kind))
                .filter(|message| {
                    query.min_created_at.is_none_or(|min_at| {
                        message.created_at >= min_at || message.updated_at >= min_at
                    })
                })
                .filter(|message| {
                    query
                        .max_created_at
                        .is_none_or(|max_at| message.created_at <= max_at)
                })
                .cloned()
                .collect::<Vec<_>>();

            filtered.sort_by(|a, b| {
                let ordering = a
                    .created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.id.cmp(&b.id));
                match query.sort_order {
                    SortOrder::Asc => ordering,
                    SortOrder::Desc => ordering.reverse(),
                }
            });

            let total = filtered.len() as i64;
            let offset = query.offset.unwrap_or(0);
            let limit = query.limit.unwrap_or(100);
            Ok((
                filtered.into_iter().skip(offset).take(limit).collect(),
                total,
            ))
        }

        fn list_channels(&self) -> Result<Vec<ChannelSummary>> {
            let messages = self.messages.lock().expect("store mutex poisoned");
            let mut by_channel = BTreeMap::<String, (i64, i64)>::new();
            for message in messages.iter() {
                let entry = by_channel
                    .entry(message.channel.clone())
                    .or_insert((0, message.created_at));
                entry.0 += 1;
                entry.1 = entry.1.max(message.created_at);
            }
            let mut channels = by_channel
                .into_iter()
                .map(
                    |(channel, (message_count, last_message_at))| ChannelSummary {
                        channel,
                        message_count,
                        last_message_at,
                    },
                )
                .collect::<Vec<_>>();
            channels.sort_by(|a, b| {
                b.last_message_at
                    .cmp(&a.last_message_at)
                    .then_with(|| a.channel.cmp(&b.channel))
            });
            Ok(channels)
        }
    }

    impl AuthStore for MockStore {
        fn create_mcp_token(
            &self,
            token: crate::db::NewMcpToken,
        ) -> Result<crate::db::McpTokenRecord> {
            let record = crate::db::McpTokenRecord {
                id: token.id.clone(),
                name: token.name.clone(),
                subject: token.subject.clone(),
                created_at: token.created_at,
                last_used_at: None,
                expires_at: token.expires_at,
            };
            self.mcp_token_hashes
                .lock()
                .expect("store mutex poisoned")
                .insert(token.token_hash, token.id);
            self.mcp_tokens
                .lock()
                .expect("store mutex poisoned")
                .push(record.clone());
            Ok(record)
        }

        fn find_mcp_token_by_hash(&self, hash: &str) -> Result<Option<crate::db::McpTokenRecord>> {
            let hashes = self.mcp_token_hashes.lock().expect("store mutex poisoned");
            let tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            Ok(hashes
                .get(hash)
                .and_then(|id| tokens.iter().find(|record| record.id == *id).cloned()))
        }

        fn touch_mcp_token(&self, id: &str, now: i64) -> Result<()> {
            let mut tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            for record in tokens.iter_mut() {
                if record.id == id {
                    record.last_used_at = Some(now);
                    break;
                }
            }
            Ok(())
        }

        fn list_mcp_tokens(&self, subject: Option<&str>) -> Result<Vec<crate::db::McpTokenRecord>> {
            let tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            Ok(tokens
                .iter()
                .filter(|record| match subject {
                    Some(subject) => record.subject.as_deref() == Some(subject),
                    None => true,
                })
                .cloned()
                .collect())
        }

        fn delete_mcp_token(&self, id: &str, subject: Option<&str>) -> Result<bool> {
            let mut tokens = self.mcp_tokens.lock().expect("store mutex poisoned");
            let before = tokens.len();
            tokens.retain(|record| {
                record.id != id
                    || match subject {
                        Some(subject) => record.subject.as_deref() != Some(subject),
                        None => false,
                    }
            });
            Ok(tokens.len() != before)
        }

        fn create_device_auth(
            &self,
            request: crate::db::NewDeviceAuth,
        ) -> Result<crate::db::DeviceAuthRecord> {
            let record = crate::db::DeviceAuthRecord {
                device_code: request.device_code,
                user_code: request.user_code,
                status: crate::db::DeviceAuthStatus::Pending,
                token_id: None,
                subject: None,
                client_name: request.client_name,
                created_at: request.created_at,
                expires_at: request.expires_at,
                interval_secs: request.interval_secs,
                last_polled_at: None,
            };
            self.device_auths
                .lock()
                .expect("store mutex poisoned")
                .push(record.clone());
            Ok(record)
        }

        fn find_device_auth_by_device_code(
            &self,
            device_code: &str,
        ) -> Result<Option<crate::db::DeviceAuthRecord>> {
            Ok(self
                .device_auths
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .find(|record| record.device_code == device_code)
                .cloned())
        }

        fn find_device_auth_by_user_code(
            &self,
            user_code: &str,
        ) -> Result<Option<crate::db::DeviceAuthRecord>> {
            Ok(self
                .device_auths
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .find(|record| record.user_code == user_code)
                .cloned())
        }

        fn approve_device_auth(
            &self,
            user_code: &str,
            token_id: &str,
            subject: Option<&str>,
            now: i64,
        ) -> Result<bool> {
            let mut auths = self.device_auths.lock().expect("store mutex poisoned");
            for record in auths.iter_mut() {
                if record.user_code == user_code
                    && matches!(record.status, crate::db::DeviceAuthStatus::Pending)
                    && record.expires_at > now
                {
                    record.status = crate::db::DeviceAuthStatus::Approved;
                    record.token_id = Some(token_id.to_owned());
                    record.subject = subject.map(str::to_owned);
                    return Ok(true);
                }
            }
            Ok(false)
        }

        fn touch_device_poll(&self, device_code: &str, now: i64) -> Result<()> {
            let mut auths = self.device_auths.lock().expect("store mutex poisoned");
            for record in auths.iter_mut() {
                if record.device_code == device_code {
                    record.last_polled_at = Some(now);
                    break;
                }
            }
            Ok(())
        }

        fn expire_device_auths(&self, now: i64) -> Result<usize> {
            let mut auths = self.device_auths.lock().expect("store mutex poisoned");
            let mut expired = 0;
            for record in auths.iter_mut() {
                if matches!(record.status, crate::db::DeviceAuthStatus::Pending)
                    && record.expires_at <= now
                {
                    record.status = crate::db::DeviceAuthStatus::Expired;
                    expired += 1;
                }
            }
            Ok(expired)
        }

        fn create_auth_code(
            &self,
            code: crate::db::NewOAuthAuthCode,
        ) -> Result<crate::db::OAuthAuthCodeRecord> {
            let record = crate::db::OAuthAuthCodeRecord {
                code: code.code,
                client_id: code.client_id,
                redirect_uri: code.redirect_uri,
                code_challenge: code.code_challenge,
                challenge_method: code.challenge_method,
                scope: code.scope,
                subject: code.subject,
                token_id: None,
                created_at: code.created_at,
                expires_at: code.expires_at,
                consumed_at: None,
            };
            self.auth_codes
                .lock()
                .expect("store mutex poisoned")
                .push(record.clone());
            Ok(record)
        }

        fn find_auth_code(&self, code: &str) -> Result<Option<crate::db::OAuthAuthCodeRecord>> {
            Ok(self
                .auth_codes
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .find(|record| record.code == code)
                .cloned())
        }

        fn consume_auth_code(&self, code: &str, token_id: &str, now: i64) -> Result<bool> {
            let mut codes = self.auth_codes.lock().expect("store mutex poisoned");
            for record in codes.iter_mut() {
                if record.code == code && record.consumed_at.is_none() && record.expires_at > now {
                    record.consumed_at = Some(now);
                    record.token_id = Some(token_id.to_owned());
                    return Ok(true);
                }
            }
            Ok(false)
        }

        fn expire_auth_codes(&self, now: i64) -> Result<usize> {
            let mut codes = self.auth_codes.lock().expect("store mutex poisoned");
            let before = codes.len();
            codes.retain(|record| record.expires_at > now);
            Ok(before - codes.len())
        }
    }

    fn manual_edge(id: &str, from: &str, to: &str) -> GraphEdgeRecord {
        GraphEdgeRecord {
            id: id.to_owned(),
            from_item_id: from.to_owned(),
            to_item_id: to.to_owned(),
            edge_type: GraphEdgeType::Manual,
            relation: Some("supports".to_owned()),
            sort_order: crate::db::format_edge_sort_order(1024),
            weight: 1.0,
            directed: true,
            metadata: json!({"kind": "manual"}),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn similarity_edge(id: &str, from: &str, to: &str) -> GraphEdgeRecord {
        GraphEdgeRecord {
            id: id.to_owned(),
            from_item_id: from.to_owned(),
            to_item_id: to.to_owned(),
            edge_type: GraphEdgeType::Similarity,
            relation: None,
            sort_order: crate::db::format_edge_sort_order(2048),
            weight: 0.9,
            directed: false,
            metadata: json!({"distance": 0.2}),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn store_route_embeds_and_persists_payload() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.25, 0.75]));
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/store")
            .json(&json!({
                "id": "doc-1",
                "text": "hello world",
                "metadata": { "source": "unit-test" },
                "source_id": "knowledge"
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body = response.json::<StoreResponse>();
        assert_eq!(body.id, "doc-1");
        assert_eq!(body.source_id, "knowledge");
        assert!(body.created_at > 0);

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0.id, "doc-1");
        assert_eq!(stored[0].0.metadata, json!({ "source": "unit-test" }));
        assert_eq!(stored[0].0.source_id, "knowledge");
        assert!(stored[0].0.created_at > 0);
        assert_eq!(stored[0].1, vec![0.25, 0.75]);
    }

    #[tokio::test]
    async fn store_route_generates_id_when_missing() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.25, 0.75]));
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/store")
            .json(&json!({
                "text": "hello world",
                "metadata": { "source": "unit-test" },
                "source_id": "knowledge"
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body = response.json::<StoreResponse>();
        assert!(!body.id.trim().is_empty());
        assert_eq!(body.source_id, "knowledge");

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0.id, body.id);
    }

    #[tokio::test]
    async fn store_route_generates_id_when_blank() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.25, 0.75]));
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/store")
            .json(&json!({
                "id": "   ",
                "text": "hello world",
                "metadata": { "source": "unit-test" },
                "source_id": "knowledge"
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body = response.json::<StoreResponse>();
        assert!(!body.id.trim().is_empty());

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0.id, body.id);
    }

    #[tokio::test]
    async fn search_route_returns_ranked_results() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![SearchHit {
            id: "doc-7".to_owned(),
            text: "stored text".to_owned(),
            metadata: json!({ "label": "match" }),
            source_id: "memory".to_owned(),
            created_at: 1234,
            updated_at: 1234,
            distance: 0.0125,
            section_path: Vec::new(),
            retrievers: Vec::new(),
            chunk_text: None,
            path: None,
            type_name: None,
            tags: Vec::new(),
            analysis: None,
        }]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .post("/search")
            .json(&json!({
                "query": "hello",
                "top_k": 1,
                "source_id": "memory"
            }))
            .await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "results": [{
                "id": "doc-7",
                "text": "stored text",
                "metadata": { "label": "match" },
                "source_id": "memory",
                "created_at": 1234,
                "distance": 0.0125
            }],
            "related": []
        }));

        let search_source_ids = store
            .search_source_ids
            .lock()
            .expect("store mutex poisoned");
        assert_eq!(search_source_ids.as_slice(), &[Some("memory".to_owned())]);
    }

    #[tokio::test]
    async fn search_route_filters_by_max_distance() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![
            SearchHit {
                id: "doc-near".to_owned(),
                text: "close".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 1,
                distance: 0.3,
                section_path: Vec::new(),
                retrievers: Vec::new(),
                chunk_text: None,
                path: None,
                type_name: None,
                tags: Vec::new(),
                analysis: None,
                updated_at: 1,
            },
            SearchHit {
                id: "doc-far".to_owned(),
                text: "far".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 2,
                distance: 1.5,
                section_path: Vec::new(),
                retrievers: Vec::new(),
                chunk_text: None,
                path: None,
                type_name: None,
                tags: Vec::new(),
                analysis: None,
                updated_at: 2,
            },
        ]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "hello", "top_k": 5 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 1);
        assert_eq!(body.results[0].id, "doc-near");
    }

    #[tokio::test]
    async fn search_route_returns_related_manual_neighbors_of_top_hit() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore {
            stored: Mutex::new(vec![
                (
                    ItemRecord {
                        id: "doc-top".to_owned(),
                        text: "kubernetes ingress".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 1,
                        updated_at: 1,
                        path: None,
                        type_name: None,
                        data: None,
                        analysis: None,
                    },
                    Vec::new(),
                ),
                (
                    ItemRecord {
                        id: "doc-linked".to_owned(),
                        text: "kubernetes storage".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 2,
                        updated_at: 2,
                        path: None,
                        type_name: None,
                        data: None,
                        analysis: None,
                    },
                    Vec::new(),
                ),
                (
                    ItemRecord {
                        id: "doc-similar".to_owned(),
                        text: "sim neighbor".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 3,
                        updated_at: 3,
                        path: None,
                        type_name: None,
                        data: None,
                        analysis: None,
                    },
                    Vec::new(),
                ),
            ]),
            messages: Mutex::new(Vec::new()),
            search_results: Mutex::new(vec![SearchHit {
                id: "doc-top".to_owned(),
                text: "kubernetes ingress".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 1,
                updated_at: 1,
                distance: 0.2,
                section_path: Vec::new(),
                retrievers: Vec::new(),
                chunk_text: None,
                path: None,
                type_name: None,
                tags: Vec::new(),
                analysis: None,
            }]),
            search_source_ids: Mutex::new(Vec::new()),
            graph_enabled: true,
            graph_edges: Mutex::new(vec![
                manual_edge("manual-1", "doc-top", "doc-linked"),
                similarity_edge("sim-1", "doc-top", "doc-similar"),
            ]),
            graph_rebuilds: Mutex::new(0),
            mcp_tokens: Mutex::new(Vec::new()),
            mcp_token_hashes: Mutex::new(HashMap::new()),
            device_auths: Mutex::new(Vec::new()),
            auth_codes: Mutex::new(Vec::new()),
        });
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "kubernetes ingress", "top_k": 5 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 1);
        assert_eq!(body.results[0].id, "doc-top");
        assert_eq!(body.related.len(), 1);
        assert_eq!(body.related[0].id, "doc-linked");
        assert_eq!(body.related[0].relation.as_deref(), Some("supports"));
    }

    #[tokio::test]
    async fn search_route_defaults_top_k_when_omitted() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![SearchHit {
            id: "doc-1".to_owned(),
            text: "hit".to_owned(),
            metadata: json!({}),
            source_id: "memory".to_owned(),
            created_at: 1,
            updated_at: 1,
            distance: 0.1,
            section_path: Vec::new(),
            retrievers: Vec::new(),
            chunk_text: None,
            path: None,
            type_name: None,
            tags: Vec::new(),
            analysis: None,
        }]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "hello" }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 1);
    }

    #[tokio::test]
    async fn search_route_excludes_related_already_in_results() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore {
            stored: Mutex::new(vec![
                (
                    ItemRecord {
                        id: "doc-top".to_owned(),
                        text: "top".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 1,
                        updated_at: 1,
                        path: None,
                        type_name: None,
                        data: None,
                        analysis: None,
                    },
                    Vec::new(),
                ),
                (
                    ItemRecord {
                        id: "doc-linked".to_owned(),
                        text: "linked".to_owned(),
                        metadata: json!({}),
                        source_id: "memory".to_owned(),
                        created_at: 2,
                        updated_at: 2,
                        path: None,
                        type_name: None,
                        data: None,
                        analysis: None,
                    },
                    Vec::new(),
                ),
            ]),
            messages: Mutex::new(Vec::new()),
            search_results: Mutex::new(vec![
                SearchHit {
                    id: "doc-top".to_owned(),
                    text: "top".to_owned(),
                    metadata: json!({}),
                    source_id: "memory".to_owned(),
                    created_at: 1,
                    updated_at: 1,
                    distance: 0.1,
                    section_path: Vec::new(),
                    retrievers: Vec::new(),
                    chunk_text: None,
                    path: None,
                    type_name: None,
                    tags: Vec::new(),
                    analysis: None,
                },
                SearchHit {
                    id: "doc-linked".to_owned(),
                    text: "linked".to_owned(),
                    metadata: json!({}),
                    source_id: "memory".to_owned(),
                    created_at: 2,
                    updated_at: 2,
                    distance: 0.4,
                    section_path: Vec::new(),
                    retrievers: Vec::new(),
                    chunk_text: None,
                    path: None,
                    type_name: None,
                    tags: Vec::new(),
                    analysis: None,
                },
            ]),
            search_source_ids: Mutex::new(Vec::new()),
            graph_enabled: true,
            graph_edges: Mutex::new(vec![manual_edge("manual-1", "doc-top", "doc-linked")]),
            graph_rebuilds: Mutex::new(0),
            mcp_tokens: Mutex::new(Vec::new()),
            mcp_token_hashes: Mutex::new(HashMap::new()),
            device_auths: Mutex::new(Vec::new()),
            auth_codes: Mutex::new(Vec::new()),
        });
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "top", "top_k": 5 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 2);
        assert!(body.related.is_empty(), "doc-linked is already in results");
    }

    #[tokio::test]
    async fn search_route_respects_custom_max_distance() {
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2, 0.3]));
        let store = Arc::new(MockStore::with_results(vec![
            SearchHit {
                id: "doc-near".to_owned(),
                text: "close".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 1,
                distance: 0.3,
                section_path: Vec::new(),
                retrievers: Vec::new(),
                chunk_text: None,
                path: None,
                type_name: None,
                tags: Vec::new(),
                analysis: None,
                updated_at: 1,
            },
            SearchHit {
                id: "doc-far".to_owned(),
                text: "far".to_owned(),
                metadata: json!({}),
                source_id: "memory".to_owned(),
                created_at: 2,
                distance: 1.5,
                section_path: Vec::new(),
                retrievers: Vec::new(),
                chunk_text: None,
                path: None,
                type_name: None,
                tags: Vec::new(),
                analysis: None,
                updated_at: 2,
            },
        ]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .post("/search")
            .json(&json!({ "query": "hello", "top_k": 5, "max_distance": 2.0 }))
            .await;

        response.assert_status_ok();
        let body = response.json::<SearchResponse>();
        assert_eq!(body.results.len(), 2);
    }

    #[tokio::test]
    async fn graph_status_route_reports_disabled_state() {
        let store = Arc::new(MockStore::seed(vec![]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/graph/status").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "enabled": false,
            "build_on_startup": false,
            "similarity_top_k": 5,
            "similarity_max_distance": 0.75,
            "cross_source": false,
            "item_count": 0,
            "edge_count": 0,
            "similarity_edge_count": 0,
            "manual_edge_count": 0
        }));
    }

    #[tokio::test]
    async fn graph_neighborhood_route_returns_nodes_and_edges() {
        let store = Arc::new(MockStore::seed_graph(
            vec![
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "one".to_owned(),
                    metadata: json!({"kind":"a"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 100,
                    updated_at: 100,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                },
                ItemRecord {
                    id: "doc-2".to_owned(),
                    text: "two".to_owned(),
                    metadata: json!({"kind":"b"}),
                    source_id: "memory".to_owned(),
                    created_at: 200,
                    updated_at: 200,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                },
                ItemRecord {
                    id: "doc-3".to_owned(),
                    text: "three".to_owned(),
                    metadata: json!({"kind":"c"}),
                    source_id: "memory".to_owned(),
                    created_at: 300,
                    updated_at: 300,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                },
            ],
            vec![
                similarity_edge("sim-1", "doc-2", "doc-3"),
                manual_edge("manual-1", "doc-2", "doc-1"),
            ],
        ));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server
            .get("/graph/neighborhood/doc-2?depth=1&limit=10")
            .await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "center_id": "doc-2",
            "nodes": [
                {
                    "id": "doc-2",
                    "text": "two",
                    "metadata": {"kind":"b"},
                    "source_id": "memory",
                    "created_at": 200
                },
                {
                    "id": "doc-3",
                    "text": "three",
                    "metadata": {"kind":"c"},
                    "source_id": "memory",
                    "created_at": 300
                },
                {
                    "id": "doc-1",
                    "text": "one",
                    "metadata": {"kind":"a"},
                    "source_id": "knowledge",
                    "created_at": 100
                }
            ],
            "edges": [
                {
                    "id": "manual-1",
                    "from_item_id": "doc-2",
                    "to_item_id": "doc-1",
                    "edge_type": "manual",
                    "relation": "supports",
                    "weight": 1.0,
                    "directed": true,
                    "metadata": {"kind":"manual"},
                    "created_at": 1,
                    "updated_at": 1
                },
                {
                    "id": "sim-1",
                    "from_item_id": "doc-2",
                    "to_item_id": "doc-3",
                    "edge_type": "similarity",
                    "relation": null,
                    "weight": 0.9,
                    "directed": false,
                    "metadata": {"distance":0.2},
                    "created_at": 1,
                    "updated_at": 1
                }
            ],
            "pairwise_distances": []
        }));
    }

    #[tokio::test]
    async fn create_and_delete_manual_edge_routes_work() {
        let store = Arc::new(MockStore::seed_graph(
            vec![
                ItemRecord {
                    id: "doc-1".to_owned(),
                    text: "one".to_owned(),
                    metadata: json!({"kind":"a"}),
                    source_id: "knowledge".to_owned(),
                    created_at: 100,
                    updated_at: 100,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                },
                ItemRecord {
                    id: "doc-2".to_owned(),
                    text: "two".to_owned(),
                    metadata: json!({"kind":"b"}),
                    source_id: "memory".to_owned(),
                    created_at: 200,
                    updated_at: 200,
                    path: None,
                    type_name: None,
                    data: None,
                    analysis: None,
                },
            ],
            vec![],
        ));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let created = server
            .post("/admin/graph/edges")
            .json(&json!({
                "from_item_id": "doc-2",
                "to_item_id": "doc-1",
                "relation": "supports",
                "metadata": { "kind": "manual" }
            }))
            .await;

        assert_eq!(created.status_code(), StatusCode::CREATED);
        let created_body = created.json::<GraphEdgePayload>();
        assert_eq!(created_body.edge_type, GraphEdgeType::Manual);

        let deleted = server
            .delete(&format!("/admin/graph/edges/{}", created_body.id))
            .await;

        deleted.assert_status_ok();
        deleted.assert_json(&json!({
            "id": created_body.id,
            "deleted": true
        }));
        assert!(
            store
                .graph_edges
                .lock()
                .expect("store mutex poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn rebuild_graph_route_returns_edge_count() {
        let store = Arc::new(MockStore::seed_graph(
            vec![ItemRecord {
                id: "doc-1".to_owned(),
                text: "one".to_owned(),
                metadata: json!({"kind":"a"}),
                source_id: "knowledge".to_owned(),
                created_at: 100,
                updated_at: 100,
                path: None,
                type_name: None,
                data: None,
                analysis: None,
            }],
            vec![similarity_edge("sim-1", "doc-1", "doc-2")],
        ));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server.post("/admin/graph/rebuild").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "rebuilt_edges": 1
        }));
        assert_eq!(
            *store.graph_rebuilds.lock().expect("store mutex poisoned"),
            1
        );
    }

    #[tokio::test]
    async fn list_categories_route_returns_category_counts() {
        let store = Arc::new(MockStore::seed(vec![
            ItemRecord {
                id: "doc-1".to_owned(),
                text: "one".to_owned(),
                metadata: json!({"kind":"a"}),
                source_id: "knowledge".to_owned(),
                created_at: 100,
                updated_at: 100,
                path: None,
                type_name: None,
                data: None,
                analysis: None,
            },
            ItemRecord {
                id: "doc-2".to_owned(),
                text: "two".to_owned(),
                metadata: json!({"kind":"b"}),
                source_id: "memory".to_owned(),
                created_at: 200,
                updated_at: 200,
                path: None,
                type_name: None,
                data: None,
                analysis: None,
            },
            ItemRecord {
                id: "doc-3".to_owned(),
                text: "three".to_owned(),
                metadata: json!({"kind":"c"}),
                source_id: "memory".to_owned(),
                created_at: 300,
                updated_at: 300,
                path: None,
                type_name: None,
                data: None,
                analysis: None,
            },
        ]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/categories").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "categories": [
                { "source_id": "knowledge", "item_count": 1 },
                { "source_id": "memory", "item_count": 2 }
            ]
        }));
    }

    #[tokio::test]
    async fn list_items_route_can_filter_by_category() {
        let store = Arc::new(MockStore::seed(vec![
            ItemRecord {
                id: "doc-1".to_owned(),
                text: "one".to_owned(),
                metadata: json!({"kind":"a"}),
                source_id: "knowledge".to_owned(),
                created_at: 100,
                updated_at: 100,
                path: None,
                type_name: None,
                data: None,
                analysis: None,
            },
            ItemRecord {
                id: "doc-2".to_owned(),
                text: "two".to_owned(),
                metadata: json!({"kind":"b"}),
                source_id: "memory".to_owned(),
                created_at: 200,
                updated_at: 200,
                path: None,
                type_name: None,
                data: None,
                analysis: None,
            },
        ]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/items?source_id=memory").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "items": [{
                "id": "doc-2",
                "text": "two",
                "metadata": {"kind":"b"},
                "source_id": "memory",
                "created_at": 200
            }],
            "total_count": 1
        }));
    }

    #[tokio::test]
    async fn get_item_route_returns_full_entry() {
        let store = Arc::new(MockStore::seed(vec![ItemRecord {
            id: "doc-1".to_owned(),
            text: "full content".to_owned(),
            metadata: json!({ "kind": "reference" }),
            source_id: "knowledge".to_owned(),
            created_at: 42,
            updated_at: 42,
            path: None,
            type_name: None,
            data: None,
            analysis: None,
        }]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.0]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/items/doc-1").await;
        response.assert_status_ok();
        response.assert_json(&json!({
            "id": "doc-1",
            "text": "full content",
            "metadata": { "kind": "reference" },
            "source_id": "knowledge",
            "created_at": 42
        }));
    }

    #[tokio::test]
    async fn get_item_route_returns_404_when_missing() {
        let store = Arc::new(MockStore::seed(vec![]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.0]));
        let server = TestServer::new(router(AppState::new_ready(embedder, store.clone(), store)));

        let response = server.get("/admin/items/nope").await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_item_route_reembeds_and_preserves_created_at() {
        let store = Arc::new(MockStore::seed(vec![ItemRecord {
            id: "doc-1".to_owned(),
            text: "old".to_owned(),
            metadata: json!({"kind":"old"}),
            source_id: "knowledge".to_owned(),
            created_at: 123,
            updated_at: 123,
            path: None,
            type_name: None,
            data: None,
            analysis: None,
        }]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.9, 0.1]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server
            .put("/admin/items/doc-1")
            .json(&json!({
                "text": "new text",
                "metadata": { "kind": "new" },
                "source_id": "memory"
            }))
            .await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "id": "doc-1",
            "text": "new text",
            "metadata": { "kind": "new" },
            "source_id": "memory",
            "created_at": 123
        }));

        let stored = store.stored.lock().expect("store mutex poisoned");
        assert_eq!(stored[0].0.source_id, "memory");
        assert_eq!(stored[0].0.created_at, 123);
        assert_eq!(stored[0].1, vec![0.9, 0.1]);
    }

    #[tokio::test]
    async fn delete_item_route_removes_item() {
        let store = Arc::new(MockStore::seed(vec![ItemRecord {
            id: "doc-1".to_owned(),
            text: "old".to_owned(),
            metadata: json!({"kind":"old"}),
            source_id: "knowledge".to_owned(),
            created_at: 123,
            updated_at: 123,
            path: None,
            type_name: None,
            data: None,
            analysis: None,
        }]));
        let embedder = Arc::new(MockEmbedder::new(vec![0.9, 0.1]));
        let server = TestServer::new(router(AppState::new_ready(
            embedder,
            store.clone(),
            store.clone(),
        )));

        let response = server.delete("/admin/items/doc-1").await;

        response.assert_status_ok();
        response.assert_json(&json!({
            "id": "doc-1",
            "deleted": true
        }));
        assert!(
            store
                .stored
                .lock()
                .expect("store mutex poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn health_route_reports_loading_state() {
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new(
            Arc::new(EmbedderHandle::loading()),
            store.clone(),
            store,
            Arc::new(NoopUserMemory),
            Arc::new(NoopMessages),
            AuthConfig::default(),
            OpenAiChatConfig {
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
            MultimodalConfig::default(),
            "uploads".to_owned(),
            ChunkingConfig::default(),
        )));

        let response = server.get("/healthz").await;

        assert_eq!(response.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        response.assert_json(&json!({
            "status": "loading",
            "error": null
        }));
    }

    #[tokio::test]
    async fn openai_chat_route_returns_unauthorized_when_api_key_is_missing() {
        let store = Arc::new(MockStore::default());
        let server = TestServer::new(router(AppState::new(
            Arc::new(EmbedderHandle::loading()),
            store.clone(),
            store,
            Arc::new(NoopUserMemory),
            Arc::new(NoopMessages),
            AuthConfig {
                enabled: true,
                frontend_api_key: Some("expected-key".to_owned()),
                ..AuthConfig::default()
            },
            OpenAiChatConfig {
                base_url: Some("http://127.0.0.1:8081".to_owned()),
                default_model: Some("current_model.gguf".to_owned()),
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
            MultimodalConfig::default(),
            "uploads".to_owned(),
            ChunkingConfig::default(),
        )));

        let response = server
            .post("/api/openai/v1/chat/completions")
            .json(&json!({
                "messages": [
                    { "role": "user", "content": "hello" }
                ],
                "stream": true
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
        response.assert_json(&json!({
            "error": "missing x-api-key header, bearer token or valid session cookie"
        }));
    }

    fn mint_session_cookie(secret: &str, sub: &str) -> String {
        use jsonwebtoken::{EncodingKey, Header, encode};

        #[derive(serde::Serialize)]
        struct Claims<'a> {
            sub: &'a str,
            exp: usize,
        }

        let exp = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600) as usize;
        let token = encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &Claims { sub, exp },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();
        format!("rag_session={token}")
    }

    fn auth_test_state() -> (AppState, Arc<MockStore>) {
        let store = Arc::new(MockStore::default());
        let state = auth_test_state_with_store(store.clone());
        (state, store)
    }

    fn auth_test_state_with_store(store: Arc<MockStore>) -> AppState {
        let embedder: Arc<dyn EmbeddingService> = Arc::new(MockEmbedder::new(vec![0.1, 0.2]));
        let auth_store: Arc<dyn AuthStore> = store.clone();
        let vector_store: Arc<dyn VectorStore> = store.clone();
        let message_store: Arc<dyn MessageStore> = store.clone();
        AppState::new(
            Arc::new(EmbedderHandle::ready(embedder)),
            vector_store,
            auth_store,
            Arc::new(NoopUserMemory),
            message_store,
            AuthConfig {
                enabled: true,
                session_secret: Some("test-session-secret".to_owned()),
                app_base_url: Some("http://localhost:3000".to_owned()),
                device_code_ttl_secs: 120,
                device_code_interval_secs: 0,
                ..AuthConfig::default()
            },
            OpenAiChatConfig {
                timeout_secs: 60,
                ..OpenAiChatConfig::default()
            },
            MultimodalConfig::default(),
            "uploads".to_owned(),
            ChunkingConfig::default(),
        )
    }

    async fn mint_mcp_bearer(server: &TestServer, secret: &str, subject: &str) -> String {
        let code = server
            .post("/auth/device/code")
            .json(&json!({}))
            .await
            .json::<auth::DeviceCodeResponse>();
        let cookie = mint_session_cookie(secret, subject);
        server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code.user_code}))
            .await
            .assert_status_ok();
        server
            .post("/auth/device/token")
            .json(&json!({"device_code": code.device_code}))
            .await
            .json::<auth::DeviceTokenResponse>()
            .access_token
    }

    async fn initialize_mcp_session(server: &TestServer, token: &str) -> Option<String> {
        let response = server
            .post("/mcp")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {token}")
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .add_header(
                axum::http::header::HOST,
                "localhost".parse::<axum::http::HeaderValue>().unwrap(),
            )
            .add_header(
                axum::http::header::ACCEPT,
                "application/json, text/event-stream"
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "rust-rag-test", "version": "0.0.1" }
                }
            }))
            .await;
        response.assert_status_ok();
        let session_id = response
            .maybe_header("mcp-session-id")
            .map(|value| value.to_str().unwrap().to_owned());
        let body = response.json::<Value>();
        assert!(
            body.get("error").is_none(),
            "MCP initialize failed: {body:?}"
        );
        session_id
    }

    async fn call_mcp_tool(
        server: &TestServer,
        token: &str,
        session_id: Option<&str>,
        id: i64,
        name: &str,
        arguments: Value,
    ) -> Value {
        let mut request = server
            .post("/mcp")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {token}")
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .add_header(
                axum::http::header::HOST,
                "localhost".parse::<axum::http::HeaderValue>().unwrap(),
            )
            .add_header(
                axum::http::header::ACCEPT,
                "application/json, text/event-stream"
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            );
        if let Some(session_id) = session_id {
            request = request.add_header(
                axum::http::HeaderName::from_static("mcp-session-id"),
                session_id.parse::<axum::http::HeaderValue>().unwrap(),
            );
        }
        let response = request
            .json(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments
                }
            }))
            .await;
        response.assert_status_ok();
        let body = response.json::<Value>();
        assert!(body.get("error").is_none(), "MCP tool failed: {body:?}");
        body
    }

    #[tokio::test]
    async fn device_flow_end_to_end_mints_bearer_usable_on_protected_routes() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));

        let code_response = server
            .post("/auth/device/code")
            .json(&json!({"client_name": "unit-test"}))
            .await;
        code_response.assert_status_ok();
        let code_body = code_response.json::<auth::DeviceCodeResponse>();

        let pending = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code_body.device_code}))
            .await;
        assert_eq!(pending.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(
            pending.json::<Value>()["error"],
            json!("authorization_pending")
        );

        let unauth_approve = server
            .post("/auth/device/approve")
            .json(&json!({"user_code": code_body.user_code}))
            .await;
        assert_eq!(unauth_approve.status_code(), StatusCode::UNAUTHORIZED);

        let cookie = mint_session_cookie(&secret, "user-123");
        let approve = server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code_body.user_code}))
            .await;
        approve.assert_status_ok();

        let granted = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code_body.device_code}))
            .await;
        granted.assert_status_ok();
        let token_body = granted.json::<auth::DeviceTokenResponse>();
        assert!(token_body.access_token.starts_with("rag_mcp_"));

        let search = server
            .post("/search")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token_body.access_token)
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({"query": "x"}))
            .await;
        assert_ne!(
            search.status_code(),
            StatusCode::UNAUTHORIZED,
            "minted MCP token should be accepted by protected route"
        );

        let again = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code_body.device_code}))
            .await;
        assert_eq!(
            again.status_code(),
            StatusCode::BAD_REQUEST,
            "token plaintext should only be fetchable once"
        );
    }

    #[tokio::test]
    async fn mcp_endpoint_rejects_unauthenticated_requests() {
        let (state, _store) = auth_test_state();
        let server = TestServer::new(router(state));

        let unauth = server.post("/mcp").json(&json!({"jsonrpc": "2.0"})).await;
        assert_eq!(unauth.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn mcp_endpoint_accepts_authenticated_requests() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));

        let code = server
            .post("/auth/device/code")
            .json(&json!({}))
            .await
            .json::<auth::DeviceCodeResponse>();
        let cookie = mint_session_cookie(&secret, "user-mcp");
        server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code.user_code}))
            .await
            .assert_status_ok();
        let token = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code.device_code}))
            .await
            .json::<auth::DeviceTokenResponse>();

        let response = server
            .post("/mcp")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token.access_token)
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .add_header(
                axum::http::header::HOST,
                "localhost".parse::<axum::http::HeaderValue>().unwrap(),
            )
            .add_header(
                axum::http::header::ACCEPT,
                "application/json, text/event-stream"
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "rust-rag-test", "version": "0.0.1" }
                }
            }))
            .await;
        assert_ne!(
            response.status_code(),
            StatusCode::UNAUTHORIZED,
            "MCP endpoint should accept the minted bearer",
        );
    }

    #[tokio::test]
    async fn cms_tree_updates_when_mutated_via_mcp() {
        let page = ItemRecord {
            id: "page-1".to_owned(),
            text: "Landing page".to_owned(),
            metadata: json!({"title": "Landing"}),
            source_id: "project:rust-rag:knowledge".to_owned(),
            created_at: 1,
            updated_at: 1,
            path: None,
            type_name: Some("cms_page".to_owned()),
            data: Some(json!({"title": "Landing"})),
            analysis: None,
        };
        let section_a = ItemRecord {
            id: "section-a".to_owned(),
            text: "Section A".to_owned(),
            metadata: json!({"title": "Section A"}),
            source_id: "project:rust-rag:knowledge".to_owned(),
            created_at: 2,
            updated_at: 2,
            path: None,
            type_name: Some("cms_section".to_owned()),
            data: Some(json!({"title": "Section A"})),
            analysis: None,
        };
        let section_b = ItemRecord {
            id: "section-b".to_owned(),
            text: "Section B".to_owned(),
            metadata: json!({"title": "Section B"}),
            source_id: "project:rust-rag:knowledge".to_owned(),
            created_at: 3,
            updated_at: 3,
            path: None,
            type_name: Some("cms_section".to_owned()),
            data: Some(json!({"title": "Section B"})),
            analysis: None,
        };
        let leaf = ItemRecord {
            id: "leaf-1".to_owned(),
            text: "before".to_owned(),
            metadata: json!({"title": "Leaf"}),
            source_id: "project:rust-rag:knowledge".to_owned(),
            created_at: 4,
            updated_at: 4,
            path: None,
            type_name: Some("cms_markdown".to_owned()),
            data: Some(json!({"markdown": "before"})),
            analysis: None,
        };
        let store = Arc::new(MockStore::seed_graph(
            vec![page, section_a, section_b, leaf],
            vec![],
        ));
        let state = auth_test_state_with_store(store);
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let token = mint_mcp_bearer(&server, &secret, "user-cms").await;
        let session_id = initialize_mcp_session(&server, &token).await;

        let cold_tree = server
            .get("/api/cms/tree/page-1")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {token}")
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .await;
        cold_tree.assert_status_ok();
        let cold_body = cold_tree.json::<CmsTreeResponse>();
        assert!(cold_body.tree.children.is_empty());

        call_mcp_tool(
            &server,
            &token,
            session_id.as_deref(),
            2,
            "create_manual_edge",
            json!({
                "from_item_id": "page-1",
                "to_item_id": "section-b",
                "relation": "contains",
                "sort_order": "00000000000000000020",
                "directed": true,
                "metadata": {}
            }),
        )
        .await;
        call_mcp_tool(
            &server,
            &token,
            session_id.as_deref(),
            3,
            "create_manual_edge",
            json!({
                "from_item_id": "page-1",
                "to_item_id": "section-a",
                "relation": "contains",
                "sort_order": "00000000000000000010",
                "directed": true,
                "metadata": {}
            }),
        )
        .await;
        call_mcp_tool(
            &server,
            &token,
            session_id.as_deref(),
            4,
            "create_manual_edge",
            json!({
                "from_item_id": "section-a",
                "to_item_id": "leaf-1",
                "relation": "contains",
                "sort_order": "00000000000000000010",
                "directed": true,
                "metadata": {}
            }),
        )
        .await;

        let tree_with_edges = server
            .get("/api/cms/tree/page-1")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {token}")
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .await;
        tree_with_edges.assert_status_ok();
        let tree_with_edges = tree_with_edges.json::<CmsTreeResponse>();
        let child_ids = tree_with_edges
            .tree
            .children
            .iter()
            .map(|child| child.node.entry.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(child_ids, vec!["section-a", "section-b"]);
        assert_eq!(
            tree_with_edges.tree.children[0].node.children[0]
                .node
                .entry
                .text,
            "before"
        );

        call_mcp_tool(
            &server,
            &token,
            session_id.as_deref(),
            5,
            "update_item",
            json!({
                "id": "leaf-1",
                "text": "after",
                "metadata": {"title": "Leaf"},
                "source_id": "project:rust-rag:knowledge"
            }),
        )
        .await;

        let refreshed_tree = server
            .get("/api/cms/tree/page-1")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {token}")
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .await;
        refreshed_tree.assert_status_ok();
        let refreshed_tree = refreshed_tree.json::<CmsTreeResponse>();
        assert_eq!(
            refreshed_tree.tree.children[0].node.children[0]
                .node
                .entry
                .text,
            "after"
        );
    }

    #[tokio::test]
    async fn revoked_token_is_rejected() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));

        let code = server
            .post("/auth/device/code")
            .json(&json!({}))
            .await
            .json::<auth::DeviceCodeResponse>();
        let cookie = mint_session_cookie(&secret, "user-123");
        server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code.user_code}))
            .await
            .assert_status_ok();
        let token = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code.device_code}))
            .await
            .json::<auth::DeviceTokenResponse>();

        let listed = server
            .get("/api/auth/tokens")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .await;
        listed.assert_status_ok();
        let tokens = listed.json::<auth::ListTokensResponse>();
        assert_eq!(tokens.tokens.len(), 1);
        assert_eq!(tokens.tokens[0].id, token.token_id);

        server
            .delete(&format!("/api/auth/tokens/{}", token.token_id))
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .await
            .assert_status_ok();

        let search = server
            .post("/search")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token.access_token)
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({"query": "x"}))
            .await;
        assert_eq!(search.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn session_message_send_uses_authenticated_subject() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "user-123");

        let response = server
            .post("/api/messages")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({
                "channel": "general",
                "text": "hello"
            }))
            .await;

        response.assert_status(StatusCode::CREATED);
        response.assert_json_contains(&json!({
            "channel": "general",
            "sender": "user-123",
            "sender_kind": "human",
            "text": "hello"
        }));
    }

    #[tokio::test]
    async fn session_message_send_rejects_sender_override() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "user-123");

        let response = server
            .post("/api/messages")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({
                "channel": "general",
                "text": "hello",
                "sender": "mallory"
            }))
            .await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
        response.assert_json(&json!({
            "error": "session-authenticated requests cannot override sender"
        }));
    }

    #[tokio::test]
    async fn session_presence_ignores_client_supplied_user() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "user-123");

        let response = server
            .get("/api/messages?channel=general&user=mallory")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .await;

        response.assert_status_ok();
        response.assert_json_contains(&json!({
            "active_users": [
                {
                    "user": "user-123",
                    "kind": "human"
                }
            ]
        }));
    }

    #[tokio::test]
    async fn mcp_token_message_send_allows_agent_sender() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));

        let code = server
            .post("/auth/device/code")
            .json(&json!({}))
            .await
            .json::<auth::DeviceCodeResponse>();
        let cookie = mint_session_cookie(&secret, "user-mcp");
        server
            .post("/auth/device/approve")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .json(&json!({"user_code": code.user_code}))
            .await
            .assert_status_ok();
        let token = server
            .post("/auth/device/token")
            .json(&json!({"device_code": code.device_code}))
            .await
            .json::<auth::DeviceTokenResponse>();

        let response = server
            .post("/api/messages")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token.access_token)
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({
                "channel": "general",
                "text": "agent online",
                "sender": "bridge-bot",
                "sender_kind": "agent"
            }))
            .await;

        response.assert_status(StatusCode::CREATED);
        response.assert_json_contains(&json!({
            "sender": "bridge-bot",
            "sender_kind": "agent",
            "text": "agent online"
        }));
    }

    #[tokio::test]
    async fn session_cannot_delete_other_users_message() {
        let store = Arc::new(MockStore::seed_messages(vec![MessageRecord {
            id: "msg-1".to_owned(),
            channel: "general".to_owned(),
            sender: "owner-1".to_owned(),
            sender_kind: MessageSenderKind::Human,
            text: "hello".to_owned(),
            kind: "text".to_owned(),
            metadata: json!({}),
            created_at: 10,
            updated_at: 10,
        }]));
        let state = auth_test_state_with_store(store.clone());
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "user-123");

        let response = server
            .delete("/api/messages/msg-1")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .await;

        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
        assert!(
            store
                .messages
                .lock()
                .expect("store mutex poisoned")
                .iter()
                .any(|message| message.id == "msg-1")
        );
    }

    fn pkce_pair() -> (String, String) {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use sha2::{Digest, Sha256};
        let verifier = "test-verifier-with-enough-entropy-1234567890".to_owned();
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
        (verifier, challenge)
    }

    async fn run_consent(
        server: &TestServer,
        cookie: &str,
        challenge: &str,
        redirect_uri: &str,
    ) -> String {
        let resp = server
            .post("/oauth/authorize/consent")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .form(&[
                ("client_id", "test-client"),
                ("redirect_uri", redirect_uri),
                ("code_challenge", challenge),
                ("code_challenge_method", "S256"),
                ("state", "xyz"),
                ("scope", "mcp"),
            ])
            .await;
        assert_eq!(resp.status_code(), StatusCode::SEE_OTHER);
        let location = resp.header("location").to_str().unwrap().to_owned();
        // location is like "http://127.0.0.1:9999/cb?code=...&state=xyz"
        let qs = location.split_once('?').unwrap().1;
        let pairs: HashMap<String, String> = url::form_urlencoded::parse(qs.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(pairs.get("state").map(String::as_str), Some("xyz"));
        pairs.get("code").cloned().expect("code in redirect")
    }

    #[tokio::test]
    async fn pkce_flow_happy_path_mints_token() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "alice");
        let (verifier, challenge) = pkce_pair();

        let code = run_consent(&server, &cookie, &challenge, "http://127.0.0.1:9999/cb").await;

        let token_resp = server
            .post("/oauth/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "http://127.0.0.1:9999/cb"),
                ("code_verifier", &verifier),
                ("client_id", "test-client"),
            ])
            .await;
        token_resp.assert_status_ok();
        let body = token_resp.json::<Value>();
        assert_eq!(body["token_type"], json!("Bearer"));
        let access = body["access_token"].as_str().unwrap();
        assert!(access.starts_with("rag_mcp_"));

        let search = server
            .post("/search")
            .add_header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {access}")
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .json(&json!({"query": "x"}))
            .await;
        assert_ne!(search.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn pkce_flow_rejects_mismatched_verifier() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "alice");
        let (_verifier, challenge) = pkce_pair();

        let code = run_consent(&server, &cookie, &challenge, "http://127.0.0.1:9999/cb").await;

        let token_resp = server
            .post("/oauth/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "http://127.0.0.1:9999/cb"),
                ("code_verifier", "wrong-verifier-zzzzzzzzzzzzzzzzzzzzz"),
                ("client_id", "test-client"),
            ])
            .await;
        assert_eq!(token_resp.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(token_resp.json::<Value>()["error"], json!("invalid_grant"));
    }

    #[tokio::test]
    async fn pkce_flow_rejects_replayed_code() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "alice");
        let (verifier, challenge) = pkce_pair();

        let code = run_consent(&server, &cookie, &challenge, "http://127.0.0.1:9999/cb").await;

        let first = server
            .post("/oauth/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "http://127.0.0.1:9999/cb"),
                ("code_verifier", &verifier),
            ])
            .await;
        first.assert_status_ok();

        let second = server
            .post("/oauth/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "http://127.0.0.1:9999/cb"),
                ("code_verifier", &verifier),
            ])
            .await;
        assert_eq!(second.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(second.json::<Value>()["error"], json!("invalid_grant"));
    }

    #[tokio::test]
    async fn pkce_flow_rejects_redirect_uri_mismatch() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "alice");
        let (verifier, challenge) = pkce_pair();

        let code = run_consent(&server, &cookie, &challenge, "http://127.0.0.1:9999/cb").await;

        let token_resp = server
            .post("/oauth/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "http://127.0.0.1:9999/different"),
                ("code_verifier", &verifier),
            ])
            .await;
        assert_eq!(token_resp.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(token_resp.json::<Value>()["error"], json!("invalid_grant"));
    }

    #[tokio::test]
    async fn authorize_rejects_non_loopback_redirect_uri() {
        let (state, _store) = auth_test_state();
        let secret = state.auth.session_secret.clone().unwrap();
        let server = TestServer::new(router(state));
        let cookie = mint_session_cookie(&secret, "alice");
        let (_verifier, challenge) = pkce_pair();

        let resp = server
            .post("/oauth/authorize/consent")
            .add_header(
                axum::http::header::COOKIE,
                cookie.parse::<axum::http::HeaderValue>().unwrap(),
            )
            .form(&[
                ("client_id", "evil"),
                ("redirect_uri", "https://evil.example.com/cb"),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("state", "x"),
                ("scope", "mcp"),
            ])
            .await;
        assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn authorize_redirects_to_login_when_no_session() {
        let (state, _store) = auth_test_state();
        let server = TestServer::new(router(state));
        let (_verifier, challenge) = pkce_pair();

        let resp = server
            .get("/oauth/authorize")
            .add_query_params(&[
                ("response_type", "code"),
                ("client_id", "vscode"),
                ("redirect_uri", "http://127.0.0.1:9999/cb"),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("state", "abc"),
            ])
            .await;
        assert_eq!(resp.status_code(), StatusCode::SEE_OTHER);
        let location = resp.header("location").to_str().unwrap().to_owned();
        assert!(location.starts_with("/auth/login?"));
        assert!(location.contains("returnTo="));
    }

    #[tokio::test]
    async fn authorize_server_metadata_omits_device_code_grant() {
        let (state, _store) = auth_test_state();
        let server = TestServer::new(router(state));
        let resp = server.get("/.well-known/oauth-authorization-server").await;
        resp.assert_status_ok();
        let body = resp.json::<Value>();
        let grants = body["grant_types_supported"].as_array().unwrap();
        assert!(
            grants
                .iter()
                .all(|g| g.as_str() != Some("urn:ietf:params:oauth:grant-type:device_code"))
        );
        assert!(
            grants
                .iter()
                .any(|g| g.as_str() == Some("authorization_code"))
        );
        assert_eq!(body["response_types_supported"], json!(["code"]));
        assert_eq!(body["code_challenge_methods_supported"], json!(["S256"]));
        assert!(body["authorization_endpoint"].is_string());
    }
}

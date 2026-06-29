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

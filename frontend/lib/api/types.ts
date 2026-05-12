// RAG Memory & Knowledge API Types

export interface EntryMetadata {
  [key: string]: string | number | boolean | null | undefined
}

export interface Entry {
  id: string
  text: string
  metadata: EntryMetadata
  source_id: string
  created_at: number
  /** Optional: populated by endpoints that opt in to token counting. */
  token_count?: number
  /** Wiki-style hierarchical path (slash-separated), e.g. "team/handbook". */
  path?: string | null
  /** Persisted LLM-on-store analysis output. */
  analysis?: StoreAnalysis | null
  analysis_at?: number | null
  analysis_model?: string | null
  /** Structured-data type name; references a registered schema. */
  type?: string | null
  /** Typed payload conforming to the schema for `type`. */
  data?: Record<string, unknown> | null
}

export interface SchemaDefinition {
  type_name: string
  json_schema: Record<string, unknown>
  title?: string | null
  description?: string | null
  created_at: number
  updated_at: number
  item_count?: number | null
}

export interface SchemaListResponse {
  schemas: SchemaDefinition[]
}

export interface UpsertSchemaRequest {
  type_name?: string
  json_schema: Record<string, unknown>
  title?: string | null
  description?: string | null
}

export interface DeleteSchemaResponse {
  type_name: string
  deleted: boolean
  items_unset: number
}

export interface StoreAnalysisVerdict {
  target_id: string
  relation: string
  confidence: number
  reason: string
}

export interface StoreAnalysisSuggestedEdge {
  target_id: string
  rel: string
  weight: number
}

export interface StoreAnalysisQuality {
  score: number
  issues: string[]
}

export interface StoreAnalysis {
  verdicts: StoreAnalysisVerdict[]
  suggested_edges: StoreAnalysisSuggestedEdge[]
  cluster_hint?: string | null
  tags: string[]
  title?: string | null
  summary?: string | null
  doc_type?: string | null
  freshness?: string | null
  quality?: StoreAnalysisQuality | null
  raw?: string | null
}

export interface Attachment {
  id: string
  item_id: string
  filename?: string | null
  stored_name: string
  url: string
  mime?: string | null
  size?: number | null
  sha256?: string | null
  created_at: number
}

export interface AttachmentsResponse {
  attachments: Attachment[]
}

export interface TreeChild {
  segment: string
  path: string
  count: number
  has_children: boolean
}

export interface EntriesTreeResponse {
  source_id: string
  prefix: string | null
  children: TreeChild[]
  entries: Entry[]
}

export interface PathRow {
  source_id: string
  path: string
  count: number
}

export interface EntriesPathsResponse {
  paths: PathRow[]
}

export interface Category {
  id: string
  name: string
  count: number
}

export interface SearchResult {
  id: string
  text: string
  metadata: EntryMetadata
  source_id: string
  created_at: number
  score: number
  /** Header breadcrumb of the chunk that matched best (e.g. ["Architecture", "Embedding execution"]). */
  section_path?: string[]
  /** Which retrievers contributed: ["dense"] | ["sparse"] | ["dense","sparse"]. */
  retrievers?: string[]
}

export interface RelatedResult extends SearchResult {
  relation: string | null
}

export interface SearchResultsBundle {
  results: SearchResult[]
  related: RelatedResult[]
}

export interface SearchResponse {
  results: SearchResult[]
  query: string
  top_k: number
}

export interface Edge {
  id: string
  source_id: string
  target_id: string
  relationship: string
  edge_type: string
  weight: number
  directed: boolean
  distance?: number
  metadata?: EntryMetadata
}

export interface GraphNodeDistance {
  from_item_id: string
  to_item_id: string
  distance: number
}

export interface GraphNeighborhood {
  center_id: string
  nodes: Entry[]
  edges: Edge[]
  pairwise_distances: GraphNodeDistance[]
}

export interface GraphStatus {
  enabled: boolean
  build_on_startup: boolean
  similarity_top_k: number
  similarity_max_distance: number
  cross_source: boolean
  item_count: number
  edge_count: number
  similarity_edge_count: number
  manual_edge_count: number
}

export interface StoreRequest {
  id?: string
  text: string
  metadata: EntryMetadata
  source_id: string
  path?: string | null
  type?: string | null
  data?: Record<string, unknown> | null
}

export interface SearchRequest {
  query: string
  top_k?: number
  source_id?: string
  max_distance?: number
  /** Hybrid (dense + sparse RRF) when true, dense-only when false. Backend defaults to true. */
  hybrid?: boolean
  /** Cross-encoder reranking on top-N candidates. Has no effect when the server has no reranker loaded. */
  rerank?: boolean
}

export interface UpdateItemRequest {
  text: string
  metadata: EntryMetadata
  source_id: string
  path?: string | null
  type?: string | null
  data?: Record<string, unknown> | null
}

export type SortOrder = "asc" | "desc"

export interface ListItemsRequest {
  source_id?: string
  limit?: number
  offset?: number
  sort_order?: SortOrder
  path_prefix?: string
  type?: string
}

export interface RechunkRequest {
  max_chars?: number
  overlap_chars?: number
}

export interface LlmRechunkRequest {
  model?: string
  max_chunks?: number
}

export interface RechunkResponse {
  id: string
  source_id: string
  created_at: number
  chunk_ids?: string[]
}

export interface PagedItems {
  items: Entry[]
  total_count: number
}

export interface CreateEdgeRequest {
  source_id: string
  target_id: string
  relationship: string
  directed?: boolean
  weight?: number
  metadata?: EntryMetadata
}

export interface ChatCompletionToolFunction {
  name: string
  description?: string
  parameters?: Record<string, unknown>
}

export interface ChatCompletionTool {
  type: "function"
  function: ChatCompletionToolFunction
}

export interface ChatCompletionAssistantToolCall {
  id: string
  type: "function"
  function: {
    name: string
    arguments: string
  }
}

export interface ChatCompletionMessage {
  role: "system" | "user" | "assistant" | "tool"
  content?: string | Record<string, unknown> | Array<Record<string, unknown>> | null
  name?: string
  tool_call_id?: string
  tool_calls?: ChatCompletionAssistantToolCall[]
}

export interface ChatCompletionsRequest {
  model?: string
  messages: ChatCompletionMessage[]
  stream?: true
  tools?: ChatCompletionTool[]
  tool_choice?: Record<string, unknown> | string
  temperature?: number
  max_completion_tokens?: number
  parallel_tool_calls?: boolean
  [key: string]: unknown
}

export interface ChatCompletionChunkDelta {
  role?: "assistant"
  content?: string
  reasoning_content?: string
  reasoning?: string
  tool_calls?: Array<{
    index: number
    id?: string
    type?: "function"
    function?: {
      name?: string
      arguments?: string
    }
  }>
}

export interface ChatCompletionChunkChoice {
  index: number
  delta: ChatCompletionChunkDelta
  finish_reason?: string | null
}

export interface ChatCompletionChunk {
  id?: string
  object?: string
  created?: number
  model?: string
  choices: ChatCompletionChunkChoice[]
}

export interface ChatCompletionToolResult {
  object: "chat.completion.tool_result"
  tool_call_id: string
  name: string
  content: string
}

export interface ChatCompletionStreamError {
  error: {
    message: string
    type?: string
  }
}

export interface ChatCompletionStreamHandlers {
  onChunk?: (chunk: ChatCompletionChunk) => void
  onToolResult?: (result: ChatCompletionToolResult) => void
  onError?: (error: ChatCompletionStreamError) => void
  onDone?: () => void
  onEvent?: (payload: ChatCompletionChunk | ChatCompletionToolResult | ChatCompletionStreamError) => void
}

// LLM-assisted query (multi-query expansion) endpoint
export interface AssistedQueryRequest {
  query: string
  source_id?: string
  top_k?: number
  max_distance?: number
  model?: string
}

export interface AssistedQueryRawResult {
  id: string
  text: string
  metadata: EntryMetadata
  source_id: string
  created_at: number
  distance: number
}

export interface AssistedQueryQueriesEvent {
  object: "assisted_query.queries"
  queries: string[]
}

export interface AssistedQueryResultEvent {
  object: "assisted_query.result"
  query: string
  index: number
  results: AssistedQueryRawResult[]
}

export interface AssistedQueryMergedEvent {
  object: "assisted_query.merged"
  results: AssistedQueryRawResult[]
}

export type AssistedQueryEvent =
  | AssistedQueryQueriesEvent
  | AssistedQueryResultEvent
  | AssistedQueryMergedEvent
  | ChatCompletionStreamError

export type MessageSenderKind = "human" | "agent" | "system"

export type MessageKind =
  | "text"
  | "permission_request"
  | "permission_response"
  | "tool_call"
  | "agent_chunk"
  | "agent_root_discovery"
  | string

export interface PermissionOption {
  option_id: string
  name: string
  kind?: string
}

export interface PermissionRequestMetadata {
  request_id: string
  options: PermissionOption[]
  tool_call?: {
    title?: string
    kind?: string
    raw_input?: unknown
  }
  /** When set, indicates the request has been resolved (mirrors response option_id). */
  resolved_option_id?: string
}

export interface PermissionResponseMetadata {
  request_id: string
  option_id: string
}

export interface AgentRootDiscoveryMetadata {
  root: string
  folders: string[]
  agents?: string[]
}

export interface Message {
  id: string
  channel: string
  sender: string
  sender_kind: MessageSenderKind
  text: string
  kind: MessageKind
  metadata: Record<string, unknown>
  created_at: number
  updated_at: number
}

export interface UpdateMessageRequest {
  text?: string
  metadata?: Record<string, unknown>
  /** Append text to existing body instead of replacing it. */
  append?: boolean
}

export interface MessageChannel {
  channel: string
  message_count: number
  last_message_at: number
}

export interface SendMessageRequest {
  channel: string
  text?: string
  sender?: string
  sender_kind?: MessageSenderKind
  kind?: MessageKind
  metadata?: Record<string, unknown>
}

export interface ListMessagesRequest {
  channel?: string
  sender?: string
  kind?: MessageKind
  since?: number
  until?: number
  limit?: number
  offset?: number
  sort_order?: SortOrder
  user?: string
  user_kind?: MessageSenderKind
  /** Long-poll wait in seconds (max 30). */
  wait?: number
}

export interface ActiveUser {
  user: string
  kind: string
  last_seen: number
}

export interface MessagesResponse {
  messages: Message[]
  total_count: number
  active_users: ActiveUser[]
  /** Ids of messages deleted server-side since the request's `since` cursor. */
  deleted_ids: string[]
}

export interface ClearChannelResponse {
  channel: string
  deleted_count: number
}

export interface ImageIngestResponse {
  id: string
  source_id: string
  created_at: number
  source_file: string
}

export interface AssistedQueryHandlers {
  onQueries?: (event: AssistedQueryQueriesEvent) => void
  onResult?: (event: AssistedQueryResultEvent) => void
  onMerged?: (event: AssistedQueryMergedEvent) => void
  onError?: (error: ChatCompletionStreamError) => void
  onDone?: () => void
}

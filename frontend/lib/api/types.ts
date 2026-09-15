// RAG Memory & Knowledge API Types

export interface EntryMetadata {
  [key: string]: string | number | boolean | null | undefined;
}

export interface Entry {
  id: string;
  text: string;
  metadata: EntryMetadata;
  source_id: string;
  created_at: number;
  updated_at: number;
  /** Optional: populated by endpoints that opt in to token counting. */
  token_count?: number;
  /** Wiki-style hierarchical path (slash-separated), e.g. "team/handbook". */
  path?: string | null;
  /** Persisted LLM-on-store analysis output. */
  analysis?: StoreAnalysis | null;
  analysis_at?: number | null;
  analysis_model?: string | null;
  /** Structured-data type name; references a registered schema. */
  type?: string | null;
  /** Typed payload conforming to the schema for `type`. */
  data?: Record<string, unknown> | null;
  /** Contextual expansion: similar or related entries (id + title only). */
  neighbors?: EntryNeighbor[] | null;
}

export interface EntryNeighbor {
  id: string;
  title?: string | null;
  relationship?: string | null;
  source_type?: string | null;
  thumbnail?: string | null;
  repo?: string | null;
}

export interface SchemaDefinition {
  type_name: string;
  json_schema: Record<string, unknown>;
  title?: string | null;
  description?: string | null;
  created_at: number;
  updated_at: number;
  item_count?: number | null;
}

export interface SchemaListResponse {
  schemas: SchemaDefinition[];
}

export interface UpsertSchemaRequest {
  type_name?: string;
  json_schema: Record<string, unknown>;
  title?: string | null;
  description?: string | null;
}

export interface DeleteSchemaResponse {
  type_name: string;
  deleted: boolean;
  items_unset: number;
}

export interface StoreAnalysisVerdict {
  target_id: string;
  relation: string;
  confidence: number;
  reason: string;
}

export interface StoreAnalysisSuggestedEdge {
  target_id: string;
  rel: string;
  weight: number;
}

export interface StoreAnalysisQuality {
  score: number;
  issues: string[];
}

export interface StoreAnalysis {
  verdicts: StoreAnalysisVerdict[];
  suggested_edges: StoreAnalysisSuggestedEdge[];
  cluster_hint?: string | null;
  tags: string[];
  title?: string | null;
  summary?: string | null;
  doc_type?: string | null;
  freshness?: string | null;
  quality?: StoreAnalysisQuality | null;
  raw?: string | null;
}

export interface Attachment {
  id: string;
  item_id: string;
  filename?: string | null;
  stored_name: string;
  url: string;
  mime?: string | null;
  size?: number | null;
  sha256?: string | null;
  created_at: number;
}

export interface AttachmentsResponse {
  attachments: Attachment[];
}

export interface TreeChild {
  segment: string;
  path: string;
  count: number;
  has_children: boolean;
}

export interface MapPoint {
  id: string;
  x: number;
  y: number;
  z?: number;
  cluster: number;
  title?: string;
  snippet?: string;
  source_id?: string;
  path?: string;
  doc_type?: string;
  tags?: string[];
  cluster_name?: string;
  cluster_description?: string;
}

export interface EntriesTreeResponse {
  source_id: string;
  prefix: string | null;
  children: TreeChild[];
  entries: Entry[];
}

export interface PathRow {
  source_id: string;
  path: string;
  count: number;
}

export interface EntriesPathsResponse {
  paths: PathRow[];
}

export interface Category {
  id: string;
  name: string;
  count: number;
}

export interface SearchResult {
  id: string;
  text: string;
  metadata: EntryMetadata;
  source_id: string;
  created_at: number;
  updated_at?: number;
  score: number;
  /** Header breadcrumb of the chunk that matched best (e.g. ["Architecture", "Embedding execution"]). */
  section_path?: string[];
  /** Which retrievers contributed: ["dense"] | ["sparse"] | ["dense","sparse"]. */
  retrievers?: string[];
  /** Typed-entry schema name (e.g. harness_risk). Null for untyped entries. */
  type_name?: string | null;
  /** Repository name if specified. */
  repo?: string | null;
}

export interface RelatedResult extends SearchResult {
  relation: string | null;
}

export interface SearchResultsBundle {
  results: SearchResult[];
  related: RelatedResult[];
}

export interface SearchResponse {
  results: SearchResult[];
  query: string;
  top_k: number;
}

export interface Edge {
  id: string;
  source_id: string;
  target_id: string;
  /** Extracted display title of `source_id`'s entry, when it still resolves. */
  source_title?: string | null;
  /** Extracted display title of `target_id`'s entry, when it still resolves. */
  target_title?: string | null;
  relationship: string;
  edge_type: string;
  sort_order: string;
  weight: number;
  directed: boolean;
  distance?: number;
  metadata?: EntryMetadata;
}

export interface GraphNodeDistance {
  from_item_id: string;
  to_item_id: string;
  distance: number;
}

export interface GraphNeighborhood {
  center_id: string;
  nodes: Entry[];
  edges: Edge[];
  pairwise_distances: GraphNodeDistance[];
}

export interface CmsTreeChild {
  edge: Edge;
  node: CmsTreeNode;
}

export interface CmsTreeNode {
  entry: Entry;
  children: CmsTreeChild[];
}

export interface CmsTreeResponse {
  root_id: string;
  tree: CmsTreeNode;
}

export interface GraphStatus {
  enabled: boolean;
  build_on_startup: boolean;
  similarity_top_k: number;
  similarity_max_distance: number;
  cross_source: boolean;
  item_count: number;
  edge_count: number;
  similarity_edge_count: number;
  manual_edge_count: number;
}

export interface DuplicateEdgeGroup {
  from_item_id: string;
  to_item_id: string;
  edges: Edge[];
}

export interface StoreRequest {
  id?: string;
  text: string;
  metadata: EntryMetadata;
  source_id: string;
  path?: string | null;
  type?: string | null;
  data?: Record<string, unknown> | null;
  chunk?: ChunkConfig;
}

export interface ChunkConfig {
  max_chars?: number;
  overlap_chars?: number;
}

export interface IngestUrlRequest {
  url: string;
  source_id: string;
  use_cdp?: boolean;
  llm_clean?: boolean;
  path?: string;
  type?: string;
}

export interface SearchRequest {
  query: string;
  top_k?: number;
  source_id?: string;
  max_distance?: number;
  /** Hybrid (dense + sparse RRF) when true, dense-only when false. Backend defaults to true. */
  hybrid?: boolean;
  /** Cross-encoder reranking on top-N candidates. Has no effect when the server has no reranker loaded. */
  rerank?: boolean;
  type?: string;
  /** Restrict to several typed-entry schemas at once (merged per type before the top-K cut). Ignored when `type` is set. */
  type_names?: string[];
  /** Repository name filter (e.g. matst80/rust-rag). */
  repo?: string;
}

export interface UpdateItemRequest {
  text: string;
  metadata: EntryMetadata;
  source_id: string;
  path?: string | null;
  type?: string | null;
  data?: Record<string, unknown> | null;
}

export type SortOrder = "asc" | "desc";

export interface ListItemsRequest {
  source_id?: string;
  limit?: number;
  offset?: number;
  sort_order?: SortOrder;
  path_prefix?: string;
  type?: string;
  /** true = only entries with a wiki path; false = only entries with no path (unorganized). */
  has_path?: boolean;
  /** Exact-match filters on metadata fields (e.g. { author: "mats", repo: "rust-rag" }). */
  metadata?: Record<string, string>;
}

export interface RechunkRequest {
  max_chars?: number;
  overlap_chars?: number;
}

export interface LlmRechunkRequest {
  model?: string;
  max_chunks?: number;
}

export interface RechunkResponse {
  id: string;
  source_id: string;
  created_at: number;
  chunk_ids?: string[];
}

export interface PagedItems {
  items: Entry[];
  total_count: number;
}

export interface CreateEdgeRequest {
  source_id: string;
  target_id: string;
  relationship: string;
  sort_order?: string;
  directed?: boolean;
  weight?: number;
  metadata?: EntryMetadata;
}

export interface UpdateEdgeRequest {
  relation?: string;
  sort_order?: string;
  metadata: EntryMetadata;
}

export interface ChatCompletionToolFunction {
  name: string;
  description?: string;
  parameters?: Record<string, unknown>;
}

export interface ChatCompletionTool {
  type: "function";
  function: ChatCompletionToolFunction;
}

export interface ChatCompletionAssistantToolCall {
  id: string;
  type: "function";
  function: {
    name: string;
    arguments: string;
  };
}

export interface ChatCompletionMessage {
  role: "system" | "user" | "assistant" | "tool";
  content?:
    | string
    | Record<string, unknown>
    | Array<Record<string, unknown>>
    | null;
  name?: string;
  tool_call_id?: string;
  tool_calls?: ChatCompletionAssistantToolCall[];
}

export interface ChatCompletionsRequest {
  model?: string;
  messages: ChatCompletionMessage[];
  stream?: true;
  tools?: ChatCompletionTool[];
  tool_choice?: Record<string, unknown> | string;
  temperature?: number;
  max_completion_tokens?: number;
  parallel_tool_calls?: boolean;
  [key: string]: unknown;
}

export interface ChatCompletionChunkDelta {
  role?: "assistant";
  content?: string;
  reasoning_content?: string;
  reasoning?: string;
  tool_calls?: Array<{
    index: number;
    id?: string;
    type?: "function";
    function?: {
      name?: string;
      arguments?: string;
    };
  }>;
}

export interface ChatCompletionChunkChoice {
  index: number;
  delta: ChatCompletionChunkDelta;
  finish_reason?: string | null;
}

export interface ChatCompletionChunk {
  id?: string;
  object?: string;
  created?: number;
  model?: string;
  choices: ChatCompletionChunkChoice[];
}

export interface ChatCompletionToolResult {
  object: "chat.completion.tool_result";
  tool_call_id: string;
  name: string;
  content: string;
}

export interface ChatCompletionStreamError {
  error: {
    message: string;
    type?: string;
  };
}

export interface ChatCompletionStreamHandlers {
  onChunk?: (chunk: ChatCompletionChunk) => void;
  onToolResult?: (result: ChatCompletionToolResult) => void;
  onError?: (error: ChatCompletionStreamError) => void;
  onDone?: () => void;
  onEvent?: (
    payload:
      | ChatCompletionChunk
      | ChatCompletionToolResult
      | ChatCompletionStreamError,
  ) => void;
}

// LLM-assisted query (multi-query expansion) endpoint
export interface AssistedQueryRequest {
  query: string;
  source_id?: string;
  top_k?: number;
  max_distance?: number;
  model?: string;
  type?: string;
}

export interface AssistedQueryRawResult {
  id: string;
  text: string;
  metadata: EntryMetadata;
  source_id: string;
  created_at: number;
  distance: number;
}

export interface AssistedQueryQueriesEvent {
  object: "assisted_query.queries";
  queries: string[];
}

export interface AssistedQueryResultEvent {
  object: "assisted_query.result";
  query: string;
  index: number;
  results: AssistedQueryRawResult[];
}

export interface AssistedQueryMergedEvent {
  object: "assisted_query.merged";
  results: AssistedQueryRawResult[];
}

export type AssistedQueryEvent =
  | AssistedQueryQueriesEvent
  | AssistedQueryResultEvent
  | AssistedQueryMergedEvent
  | ChatCompletionStreamError;

export type MessageSenderKind = "human" | "agent" | "system";

export type MessageKind =
  | "text"
  | "permission_request"
  | "permission_response"
  | "tool_call"
  | "agent_chunk"
  | "agent_root_discovery"
  | string;

export interface PermissionOption {
  option_id: string;
  name: string;
  kind?: string;
}

export interface PermissionRequestMetadata {
  request_id: string;
  options: PermissionOption[];
  tool_call?: {
    title?: string;
    kind?: string;
    raw_input?: unknown;
  };
  /** When set, indicates the request has been resolved (mirrors response option_id). */
  resolved_option_id?: string;
}

export interface PermissionResponseMetadata {
  request_id: string;
  option_id: string;
}

export interface AgentRootDiscoveryMetadata {
  root: string;
  folders: string[];
  agents?: string[];
}

export interface Message {
  id: string;
  channel: string;
  sender: string;
  sender_kind: MessageSenderKind;
  text: string;
  kind: MessageKind;
  metadata: Record<string, unknown>;
  created_at: number;
  updated_at: number;
}

export interface UpdateMessageRequest {
  text?: string;
  metadata?: Record<string, unknown>;
  /** Append text to existing body instead of replacing it. */
  append?: boolean;
}

export interface MessageChannel {
  channel: string;
  message_count: number;
  last_message_at: number;
}

export interface SendMessageRequest {
  channel: string;
  text?: string;
  sender?: string;
  sender_kind?: MessageSenderKind;
  kind?: MessageKind;
  metadata?: Record<string, unknown>;
}

export interface ListMessagesRequest {
  channel?: string;
  sender?: string;
  kind?: MessageKind;
  since?: number;
  until?: number;
  limit?: number;
  offset?: number;
  sort_order?: SortOrder;
  user?: string;
  user_kind?: MessageSenderKind;
  /** Long-poll wait in seconds (max 30). */
  wait?: number;
}

export interface ActiveUser {
  user: string;
  kind: string;
  last_seen: number;
}

export interface MessagesResponse {
  messages: Message[];
  total_count: number;
  active_users: ActiveUser[];
  /** Ids of messages deleted server-side since the request's `since` cursor. */
  deleted_ids: string[];
}

export interface ClearChannelResponse {
  channel: string;
  deleted_count: number;
}

export interface ImageIngestResponse {
  id: string;
  source_id: string;
  created_at: number;
  source_file: string;
}

export interface AssistedQueryHandlers {
  onQueries?: (event: AssistedQueryQueriesEvent) => void;
  onResult?: (event: AssistedQueryResultEvent) => void;
  onMerged?: (event: AssistedQueryMergedEvent) => void;
  onError?: (error: ChatCompletionStreamError) => void;
  onDone?: () => void;
}
export interface DriveFile {
  id: string;
  name: string;
  mimeType: string;
  modifiedTime?: string;
  webViewLink?: string;
}

export interface DriveSearchResult {
  files: DriveFile[];
  query: string;
}

export interface FetchedDriveDoc {
  id: string;
  name: string;
  mime_type: string;
  returned_mime: string;
  content: string;
  truncated: boolean;
  size_bytes: number;
  web_view_link?: string;
}

// Code-repo ingestion
export interface CodeRepoSummary {
  name: string;
  root_path: string;
  enabled: boolean;
  file_count: number;
}

export interface CodeFileMeta {
  path: string;
  basename: string;
  language?: string | null;
  role?: string | null;
  summary?: string | null;
  size_bytes: number;
  line_count: number;
  indexed_at: number;
}

export interface CodeOutlineEntry {
  kind: string;
  name: string;
  line: number;
  signature?: string | null;
  is_public?: boolean;
  is_test?: boolean;
}

export interface CodeTodoEntry {
  kind: string;
  line: number;
  text: string;
}

export interface CodeFileDetail extends CodeFileMeta {
  outline: CodeOutlineEntry[];
  todos: CodeTodoEntry[];
  imports: string[];
}

export interface CodeSearchHit {
  repo: string;
  path: string;
  language?: string | null;
  symbol_kind?: string | null;
  symbol_name?: string | null;
  signature?: string | null;
  start_line: number;
  end_line: number;
  snippet: string;
  score: number;
}

export interface CodeSearchRequest {
  query: string;
  repo?: string;
  language?: string;
  path_prefix?: string;
  limit?: number;
}

// --- Harness (Grill) graph domain ---

export type HarnessBadge = "green" | "yellow" | "red";

export const HARNESS_NODE_TYPES = [
  "harness_doc",
  "harness_plan",
  "harness_sprint",
  "harness_todo",
  "harness_agent",
  "harness_stream",
  "harness_audit",
  "harness_repo",
  "harness_poc",
  // harness_decision folded into the generic `decision` type.
  "decision",
  "harness_risk",
  "harness_compliance",
  "harness_resource",
  "harness_scaling",
  "harness_validation",
  "harness_rollout",
  // harness_fact renamed harness_evidence (collided with the generic `fact` type).
  "harness_evidence",
] as const;

export type HarnessNodeType = (typeof HARNESS_NODE_TYPES)[number];

export interface HarnessAuditViolation {
  doc_id: string;
  title: string;
  violation_reason: string;
  severity: "BLOCKING" | "WARNING";
}

export interface HarnessSuggestedEdit {
  doc_id: string;
  suggestion: string;
}

export interface HarnessAuditVerdict {
  audit_id: string;
  passed: boolean;
  score: number;
  violations: HarnessAuditViolation[];
  unanchored_assumptions: string[];
  suggested_edits?: HarnessSuggestedEdit[];
  audited_at: number;
}

export interface HarnessTreeNode {
  id: string;
  type_name: HarnessNodeType | string;
  title: string;
  state: string | null;
  /** Full typed payload (severity, framework, phases, ...). */
  data?: Record<string, unknown> | null;
  source_id: string;
  created_at: number;
  updated_at: number;
  badge: HarnessBadge | null;
  verdict?: HarnessAuditVerdict | null;
}

export interface HarnessTreeEdge {
  id: string;
  from_item_id: string;
  to_item_id: string;
  relation: string | null;
}

/**
 * Session memory joined into the tree via harness_poc.session_id ===
 * memory.source_id. Not a `node`; join to a POC via `poc_id` and onward to
 * sprints/todos via depends_on / EVIDENCED_BY edges.
 */
export interface HarnessTreeMemory {
  id: string;
  poc_id: string;
  session_id: string;
  type_name: string | null;
  /** Entry text, truncated at 2,000 chars when `truncated` is true. */
  text: string;
  truncated: boolean;
  created_at: number;
  updated_at: number;
}

export interface HarnessTreeResponse {
  nodes: HarnessTreeNode[];
  edges: HarnessTreeEdge[];
  memories?: HarnessTreeMemory[];
}

export interface TokenCountResponse {
  token_count: number;
  char_count: number;
}

export interface ShareResponse {
  token: string;
  item_id: string;
  created_at: number;
  expires_at: number | null;
  url: string;
}

export interface CreateShareRequest {
  expires_in_seconds?: number;
}

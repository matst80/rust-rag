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
}

export interface SearchRequest {
  query: string
  top_k?: number
  source_id?: string
  max_distance?: number
}

export interface UpdateItemRequest {
  text: string
  metadata: EntryMetadata
  source_id: string
}

export type SortOrder = "asc" | "desc"

export interface ListItemsRequest {
  source_id?: string
  limit?: number
  offset?: number
  sort_order?: SortOrder
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

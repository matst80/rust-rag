import type {
  Entry,
  Category,
  SearchResult,
  RelatedResult,
  SearchResultsBundle,
  SearchRequest,
  StoreRequest,
  UpdateItemRequest,
  Edge,
  CreateEdgeRequest,
  EntryMetadata,
  GraphNeighborhood,
  GraphNodeDistance,
  GraphStatus,
  ListItemsRequest,
  LargeItemsRequest,
  RechunkRequest,
  LlmRechunkRequest,
  RechunkResponse,
  PagedItems,
  ChatCompletionChunk,
  ChatCompletionToolResult,
  ChatCompletionsRequest,
  ChatCompletionStreamError,
  ChatCompletionStreamHandlers,
  AssistedQueryRequest,
  AssistedQueryHandlers,
  AssistedQueryQueriesEvent,
  AssistedQueryResultEvent,
  AssistedQueryMergedEvent,
  ImageIngestResponse,
  Message,
  MessageChannel,
  SendMessageRequest,
  ListMessagesRequest,
  MessagesResponse,
  ClearChannelResponse,
} from "./types"

const API_BASE_URL = ""

interface CategoriesResponse {
  categories: Array<{
    source_id: string
    item_count: number
  }>
}

interface ItemsResponse {
  items: Entry[]
  total_count: number
}

interface RawSearchResult {
  id: string
  text: string
  metadata: EntryMetadata
  source_id: string
  created_at: number
  distance: number
}

interface RawRelatedResult extends RawSearchResult {
  relation: string | null
}

interface SearchResponse {
  results: RawSearchResult[]
  related?: RawRelatedResult[]
}

interface RawEdge {
  id: string
  from_item_id: string
  to_item_id: string
  edge_type: string
  relation: string | null
  weight: number
  directed: boolean
  metadata: EntryMetadata
  created_at: number
  updated_at: number
}

interface GraphEdgesResponse {
  edges: RawEdge[]
}

interface GraphNeighborhoodResponse {
  center_id: string
  nodes: Entry[]
  edges: RawEdge[]
  pairwise_distances: GraphNodeDistance[]
}

function ensureArray<T>(value: T[] | undefined, label: string): T[] {
  if (!Array.isArray(value)) {
    throw new APIError(500, `Malformed API response for ${label}`)
  }

  return value
}

function toCategory(category: CategoriesResponse["categories"][number]): Category {
  return {
    id: category.source_id,
    name: category.source_id,
    count: category.item_count,
  }
}

function toSearchResult(result: RawSearchResult): SearchResult {
  return {
    id: result.id,
    text: result.text,
    metadata: result.metadata,
    source_id: result.source_id,
    created_at: result.created_at,
    score: Math.max(0, Math.min(1, 1 - result.distance)),
  }
}

function toRelatedResult(result: RawRelatedResult): RelatedResult {
  return {
    ...toSearchResult(result),
    relation: result.relation,
  }
}

function toEdge(edge: RawEdge): Edge {
  return {
    id: edge.id,
    source_id: edge.from_item_id,
    target_id: edge.to_item_id,
    relationship: edge.relation ?? edge.edge_type,
    edge_type: edge.edge_type,
    weight: edge.weight,
    directed: edge.directed,
    distance: typeof edge.metadata?.distance === "number" ? edge.metadata.distance : undefined,
    metadata: edge.metadata,
  }
}

class APIError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message)
    this.name = "APIError"
  }
}

async function request<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${API_BASE_URL}${endpoint}`
  const response = await fetch(url, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
  })

  if (!response.ok) {
    throw new APIError(response.status, `API error: ${response.statusText}`)
  }

  // Handle empty responses (like DELETE)
  const text = await response.text()
  if (!text) return {} as T

  return JSON.parse(text)
}

function parseSseEvents(chunk: string, buffer: { current: string }): string[] {
  buffer.current += chunk.replaceAll("\r\n", "\n")
  const events: string[] = []

  while (true) {
    const separatorIndex = buffer.current.indexOf("\n\n")
    if (separatorIndex === -1) {
      break
    }

    const rawEvent = buffer.current.slice(0, separatorIndex)
    buffer.current = buffer.current.slice(separatorIndex + 2)

    const data = rawEvent
      .split("\n")
      .filter((line) => line.startsWith("data:"))
      .map((line) => line.slice(5).trimStart())
      .join("\n")

    if (data) {
      events.push(data)
    }
  }

  return events
}

function isStreamError(value: unknown): value is ChatCompletionStreamError {
  return typeof value === "object" && value !== null && "error" in value
}

function isToolResult(value: unknown): value is ChatCompletionToolResult {
  return (
    typeof value === "object" &&
    value !== null &&
    (value as any).object === "chat.completion.tool_result"
  )
}

export async function streamChatCompletions(
  input: ChatCompletionsRequest,
  handlers: ChatCompletionStreamHandlers = {},
  options: { signal?: AbortSignal } = {}
): Promise<void> {
  const response = await fetch(`${API_BASE_URL}/api/openai/v1/chat/completions`, {
    method: "POST",
    headers: {
      Accept: "text/event-stream",
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      ...input,
      stream: true,
    }),
    signal: options.signal,
  })

  if (!response.ok) {
    throw new APIError(response.status, `API error: ${response.statusText}`)
  }

  if (!response.body) {
    throw new APIError(500, "Streaming response body was not available")
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  const buffer = { current: "" }

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) {
        break
      }

      const chunk = decoder.decode(value, { stream: true })
      const events = parseSseEvents(chunk, buffer)

      for (const event of events) {
        if (event === "[DONE]") {
          handlers.onDone?.()
          return
        }

        const payload = JSON.parse(event) as
          | ChatCompletionChunk
          | ChatCompletionToolResult
          | ChatCompletionStreamError
        handlers.onEvent?.(payload)

        if (isStreamError(payload)) {
          handlers.onError?.(payload)
          continue
        }

        if (isToolResult(payload)) {
          handlers.onToolResult?.(payload)
          continue
        }

        handlers.onChunk?.(payload)
      }
    }

    handlers.onDone?.()
  } finally {
    reader.releaseLock()
  }
}

function isAssistedQueriesEvent(value: unknown): value is AssistedQueryQueriesEvent {
  return typeof value === "object" && value !== null && (value as any).object === "assisted_query.queries"
}

function isAssistedResultEvent(value: unknown): value is AssistedQueryResultEvent {
  return typeof value === "object" && value !== null && (value as any).object === "assisted_query.result"
}

function isAssistedMergedEvent(value: unknown): value is AssistedQueryMergedEvent {
  return typeof value === "object" && value !== null && (value as any).object === "assisted_query.merged"
}

export async function streamAssistedQuery(
  input: AssistedQueryRequest,
  handlers: AssistedQueryHandlers = {},
  options: { signal?: AbortSignal } = {}
): Promise<void> {
  const response = await fetch(`${API_BASE_URL}/api/query/assisted`, {
    method: "POST",
    headers: {
      Accept: "text/event-stream",
      "Content-Type": "application/json",
    },
    body: JSON.stringify(input),
    signal: options.signal,
  })

  if (!response.ok) {
    throw new APIError(response.status, `API error: ${response.statusText}`)
  }
  if (!response.body) {
    throw new APIError(500, "Streaming response body was not available")
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  const buffer = { current: "" }

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      const chunk = decoder.decode(value, { stream: true })
      const events = parseSseEvents(chunk, buffer)

      for (const event of events) {
        if (event === "[DONE]") {
          handlers.onDone?.()
          return
        }

        const payload = JSON.parse(event) as unknown
        if (isStreamError(payload)) {
          handlers.onError?.(payload)
          continue
        }
        if (isAssistedQueriesEvent(payload)) {
          handlers.onQueries?.(payload)
          continue
        }
        if (isAssistedResultEvent(payload)) {
          handlers.onResult?.(payload)
          continue
        }
        if (isAssistedMergedEvent(payload)) {
          handlers.onMerged?.(payload)
          continue
        }
      }
    }

    handlers.onDone?.()
  } finally {
    reader.releaseLock()
  }
}

// Categories API
export async function getCategories(): Promise<Category[]> {
  const response = await request<CategoriesResponse>("/admin/categories")
  return ensureArray(response.categories, "categories").map(toCategory)
}

// Items/Entries API
export async function getItems(
  options: ListItemsRequest = {}
): Promise<PagedItems> {
  const params = new URLSearchParams()
  if (options.source_id) params.append("source_id", options.source_id)
  if (options.limit !== undefined) params.append("limit", options.limit.toString())
  if (options.offset !== undefined)
    params.append("offset", options.offset.toString())
  if (options.sort_order) params.append("sort_order", options.sort_order)

  const queryString = params.toString() ? `?${params.toString()}` : ""
  const response = await request<ItemsResponse>(`/admin/items${queryString}`)
  return {
    items: ensureArray(response.items, "items"),
    total_count: response.total_count,
  }
}

export async function getItem(id: string): Promise<Entry> {
  const { items } = await getItems({ limit: 1000 }) // Hacky for now as it was before
  const item = items.find((entry) => entry.id === id)

  if (!item) {
    throw new APIError(404, `API error: item ${id} not found`)
  }

  return item
}

export async function getLargeItems(
  options: LargeItemsRequest = {}
): Promise<PagedItems> {
  const params = new URLSearchParams()
  if (options.min_chars !== undefined) params.append("min_chars", options.min_chars.toString())
  if (options.limit !== undefined) params.append("limit", options.limit.toString())
  if (options.offset !== undefined) params.append("offset", options.offset.toString())
  const queryString = params.toString() ? `?${params.toString()}` : ""
  const response = await request<{ items: Entry[]; total_count: number }>(
    `/admin/items/oversized${queryString}`
  )
  return {
    items: ensureArray(response.items, "large items"),
    total_count: response.total_count,
  }
}

export async function rechunkItem(
  id: string,
  config: RechunkRequest = {}
): Promise<RechunkResponse> {
  return request<RechunkResponse>(`/admin/items/${encodeURIComponent(id)}/rechunk`, {
    method: "POST",
    body: JSON.stringify(config),
  })
}

export async function llmRechunkItem(
  id: string,
  config: LlmRechunkRequest = {}
): Promise<RechunkResponse> {
  return request<RechunkResponse>(`/admin/items/${encodeURIComponent(id)}/llm-rechunk`, {
    method: "POST",
    body: JSON.stringify(config),
  })
}

export async function createItem(data: StoreRequest): Promise<Entry> {
  return request<Entry>("/api/store", {
    method: "POST",
    body: JSON.stringify(data),
  })
}

export async function uploadImage(
  file: File,
  sourceId: string = "images"
): Promise<ImageIngestResponse> {
  const form = new FormData()
  form.append("file", file)
  form.append("source_id", sourceId)

  const response = await fetch(`${API_BASE_URL}/api/ingest/image`, {
    method: "POST",
    body: form,
  })

  if (!response.ok) {
    throw new APIError(response.status, `Upload failed: ${response.statusText}`)
  }

  return response.json()
}

export async function updateItem(
  id: string,
  data: UpdateItemRequest
): Promise<Entry> {
  return request<Entry>(`/admin/items/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(data),
  })
}

export async function deleteItem(id: string): Promise<void> {
  await request<void>(`/admin/items/${encodeURIComponent(id)}`, {
    method: "DELETE",
  })
}

// Search API
export async function search(data: SearchRequest): Promise<SearchResultsBundle> {
  const response = await request<SearchResponse>("/api/search", {
    method: "POST",
    body: JSON.stringify({
      query: data.query,
      top_k: data.top_k ?? 10,
      ...(data.source_id && { source_id: data.source_id }),
      ...(data.max_distance !== undefined && { max_distance: data.max_distance }),
    }),
  })
  return {
    results: ensureArray(response.results, "search results").map(toSearchResult),
    related: (response.related ?? []).map(toRelatedResult),
  }
}

// Edges API
export async function getGraphStatus(): Promise<GraphStatus> {
  return request<GraphStatus>("/api/graph/status")
}

export async function getEdges(): Promise<Edge[]> {
  const response = await request<GraphEdgesResponse>("/api/graph/edges")
  return ensureArray(response.edges, "graph edges").map(toEdge)
}

export async function getEdgesForItem(itemId: string): Promise<Edge[]> {
  const response = await request<GraphEdgesResponse>(
    `/api/graph/edges?item_id=${encodeURIComponent(itemId)}`
  )
  return ensureArray(response.edges, "graph edges").map(toEdge)
}

export async function getGraphNeighborhood(
  itemId: string,
  depth: number,
  limit: number = 50
): Promise<GraphNeighborhood> {
  const params = new URLSearchParams({
    depth: String(depth),
    limit: String(limit),
  })
  const response = await request<GraphNeighborhoodResponse>(
    `/api/graph/neighborhood/${encodeURIComponent(itemId)}?${params.toString()}`
  )

  return {
    center_id: response.center_id,
    nodes: ensureArray(response.nodes, "graph neighborhood nodes"),
    edges: ensureArray(response.edges, "graph neighborhood edges").map(toEdge),
    pairwise_distances: ensureArray(
      response.pairwise_distances,
      "graph neighborhood pairwise distances"
    ),
  }
}

export async function createEdge(data: CreateEdgeRequest): Promise<Edge> {
  const response = await request<RawEdge>("/admin/graph/edges", {
    method: "POST",
    body: JSON.stringify({
      from_item_id: data.source_id,
      to_item_id: data.target_id,
      relation: data.relationship,
      directed: data.directed,
      weight: data.weight,
      metadata: data.metadata,
    }),
  })

  return toEdge(response)
}

export async function deleteEdge(id: string): Promise<void> {
  await request<void>(`/admin/graph/edges/${encodeURIComponent(id)}`, {
    method: "DELETE",
  })
}

// Messages API
export async function sendMessage(data: SendMessageRequest): Promise<Message> {
  return request<Message>("/api/messages", {
    method: "POST",
    body: JSON.stringify(data),
  })
}

export async function listMessages(
  options: ListMessagesRequest = {}
): Promise<MessagesResponse> {
  const params = new URLSearchParams()
  if (options.channel) params.append("channel", options.channel)
  if (options.sender) params.append("sender", options.sender)
  if (options.kind) params.append("kind", options.kind)
  if (options.since !== undefined) params.append("since", String(options.since))
  if (options.until !== undefined) params.append("until", String(options.until))
  if (options.limit !== undefined) params.append("limit", String(options.limit))
  if (options.offset !== undefined) params.append("offset", String(options.offset))
  if (options.sort_order) params.append("sort_order", options.sort_order)
  if (options.user) params.append("user", options.user)
  if (options.user_kind) params.append("user_kind", options.user_kind)
  if (options.wait !== undefined) params.append("wait", String(options.wait))
  const qs = params.toString() ? `?${params.toString()}` : ""
  const response = await request<MessagesResponse>(`/api/messages${qs}`)
  return {
    messages: ensureArray(response.messages, "messages"),
    total_count: response.total_count,
    active_users: response.active_users ?? [],
    deleted_ids: response.deleted_ids ?? [],
  }
}

export async function deleteMessage(id: string): Promise<void> {
  await request<void>(`/api/messages/${encodeURIComponent(id)}`, {
    method: "DELETE",
  })
}

export async function listMessageChannels(): Promise<MessageChannel[]> {
  const response = await request<{ channels: MessageChannel[] }>(
    "/api/messages/channels"
  )
  return ensureArray(response.channels, "message channels")
}

export async function clearMessageChannel(
  channel: string
): Promise<ClearChannelResponse> {
  return request<ClearChannelResponse>(
    `/api/messages/channels/${encodeURIComponent(channel)}`,
    { method: "DELETE" }
  )
}

export interface ManagerMemoryRecord {
  id: string
  kind: string
  content: string
  metadata: Record<string, unknown>
  created_at: number
  source_id: string
}

export const MANAGER_MEMORY_SOURCE_ID = "manager_memory"

export async function listManagerMemory(params?: {
  kind?: string
  search?: string
  limit?: number
}): Promise<ManagerMemoryRecord[]> {
  const limit = params?.limit ?? 100
  const { items } = await getItems({
    source_id: MANAGER_MEMORY_SOURCE_ID,
    limit,
    sort_order: "desc",
  })
  const search = params?.search?.trim().toLowerCase()
  return items
    .filter((item) => {
      const meta = (item.metadata ?? {}) as Record<string, unknown>
      if (params?.kind && meta.kind !== params.kind) return false
      if (search && !item.text.toLowerCase().includes(search)) return false
      return true
    })
    .map((item) => {
      const meta = (item.metadata ?? {}) as Record<string, unknown>
      return {
        id: item.id,
        kind: typeof meta.kind === "string" ? meta.kind : "note",
        content: item.text,
        metadata: meta,
        created_at: item.created_at,
        source_id: item.source_id,
      }
    })
}

export async function deleteManagerMemory(id: string): Promise<void> {
  await deleteItem(id)
}

export async function clearManagerMemory(
  kind?: string
): Promise<{ deleted_count: number }> {
  const memories = await listManagerMemory({ kind, limit: 1000 })
  let deleted_count = 0
  for (const m of memories) {
    try {
      await deleteItem(m.id)
      deleted_count += 1
    } catch (err) {
      console.warn("clearManagerMemory: failed to delete", m.id, err)
    }
  }
  return { deleted_count }
}

// Export API client as object
export const api = {
  categories: {
    list: getCategories,
  },
  chat: {
    stream: streamChatCompletions,
  },
  query: {
    assisted: streamAssistedQuery,
  },
  graph: {
    status: getGraphStatus,
  },
  items: {
    list: getItems,
    get: getItem,
    create: createItem,
    update: updateItem,
    delete: deleteItem,
    listLarge: getLargeItems,
    rechunk: rechunkItem,
    llmRechunk: llmRechunkItem,
    uploadImage,
  },
  search,
  messages: {
    send: sendMessage,
    list: listMessages,
    channels: listMessageChannels,
    delete: deleteMessage,
    clearChannel: clearMessageChannel,
  },
  manager: {
    memory: listManagerMemory,
    deleteMemory: deleteManagerMemory,
    clearMemory: clearManagerMemory,
  },
  edges: {
    list: getEdges,
    listForItem: getEdgesForItem,
    neighborhood: getGraphNeighborhood,
    create: createEdge,
    delete: deleteEdge,
  },
}

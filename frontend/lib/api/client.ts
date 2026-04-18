import type {
  Entry,
  Category,
  SearchResult,
  SearchRequest,
  StoreRequest,
  UpdateItemRequest,
  Edge,
  CreateEdgeRequest,
  EntryMetadata,
  GraphNeighborhood,
  GraphStatus,
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
}

interface RawSearchResult {
  id: string
  text: string
  metadata: EntryMetadata
  source_id: string
  created_at: number
  distance: number
}

interface SearchResponse {
  results: RawSearchResult[]
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

function toEdge(edge: RawEdge): Edge {
  return {
    id: edge.id,
    source_id: edge.from_item_id,
    target_id: edge.to_item_id,
    relationship: edge.relation ?? edge.edge_type,
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

// Categories API
export async function getCategories(): Promise<Category[]> {
  const response = await request<CategoriesResponse>("/admin/categories")
  return ensureArray(response.categories, "categories").map(toCategory)
}

// Items/Entries API
export async function getItems(sourceId?: string): Promise<Entry[]> {
  const params = sourceId ? `?source_id=${encodeURIComponent(sourceId)}` : ""
  const response = await request<ItemsResponse>(`/admin/items${params}`)
  return ensureArray(response.items, "items")
}

export async function getItem(id: string): Promise<Entry> {
  const items = await getItems()
  const item = items.find((entry) => entry.id === id)

  if (!item) {
    throw new APIError(404, `API error: item ${id} not found`)
  }

  return item
}

export async function createItem(data: StoreRequest): Promise<Entry> {
  return request<Entry>("/store", {
    method: "POST",
    body: JSON.stringify(data),
  })
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
export async function search(data: SearchRequest): Promise<SearchResult[]> {
  const response = await request<SearchResponse>("/search", {
    method: "POST",
    body: JSON.stringify({
      query: data.query,
      top_k: data.top_k ?? 10,
      ...(data.source_id && { source_id: data.source_id }),
      ...(data.max_distance !== undefined && { max_distance: data.max_distance }),
    }),
  })
  return ensureArray(response.results, "search results").map(toSearchResult)
}

// Edges API
export async function getGraphStatus(): Promise<GraphStatus> {
  return request<GraphStatus>("/graph/status")
}

export async function getEdges(): Promise<Edge[]> {
  const response = await request<GraphEdgesResponse>("/graph/edges")
  return ensureArray(response.edges, "graph edges").map(toEdge)
}

export async function getEdgesForItem(itemId: string): Promise<Edge[]> {
  const response = await request<GraphEdgesResponse>(
    `/graph/edges?item_id=${encodeURIComponent(itemId)}`
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
    `/graph/neighborhood/${encodeURIComponent(itemId)}?${params.toString()}`
  )

  return {
    center_id: response.center_id,
    nodes: ensureArray(response.nodes, "graph neighborhood nodes"),
    edges: ensureArray(response.edges, "graph neighborhood edges").map(toEdge),
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

// Export API client as object
export const api = {
  categories: {
    list: getCategories,
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
  },
  search,
  edges: {
    list: getEdges,
    listForItem: getEdgesForItem,
    neighborhood: getGraphNeighborhood,
    create: createEdge,
    delete: deleteEdge,
  },
}

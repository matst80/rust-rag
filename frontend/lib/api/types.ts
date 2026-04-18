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
  metadata?: EntryMetadata
}

export interface GraphNeighborhood {
  center_id: string
  nodes: Entry[]
  edges: Edge[]
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

export interface CreateEdgeRequest {
  source_id: string
  target_id: string
  relationship: string
  directed?: boolean
  weight?: number
  metadata?: EntryMetadata
}

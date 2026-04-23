import useSWR from "swr"
import useSWRMutation from "swr/mutation"
import { api } from "./client"
import type {
  Entry,
  Category,
  SearchResultsBundle,
  StoreRequest,
  UpdateItemRequest,
  Edge,
  CreateEdgeRequest,
  GraphNeighborhood,
  GraphStatus,
  ListItemsRequest,
  LargeItemsRequest,
  RechunkRequest,
  RechunkResponse,
  PagedItems,
} from "./types"

// Categories hooks
export function useCategories() {
  return useSWR<Category[]>("categories", api.categories.list)
}

// Items hooks
export function useItems(options: ListItemsRequest = {}) {
  return useSWR<PagedItems>(
    ["items", options],
    () => api.items.list(options)
  )
}

export function useItem(id: string | null) {
  return useSWR<Entry>(
    id ? ["item", id] : null,
    ([, itemId]) => api.items.get(itemId as string)
  )
}

export function useCreateItem() {
  return useSWRMutation<Entry, Error, string, StoreRequest>(
    "items",
    (_, { arg }) => api.items.create(arg)
  )
}

export function useUpdateItem(id: string) {
  return useSWRMutation<Entry, Error, string[], UpdateItemRequest>(
    ["item", id],
    (_, { arg }) => api.items.update(id, arg)
  )
}

export function useDeleteItem() {
  return useSWRMutation<void, Error, string, string>(
    "items",
    (_, { arg }) => api.items.delete(arg)
  )
}

export function useLargeItems(options: LargeItemsRequest = {}) {
  return useSWR<PagedItems>(
    ["large-items", options],
    () => api.items.listLarge(options)
  )
}

export function useRechunkItem(id: string) {
  return useSWRMutation<RechunkResponse, Error, string, RechunkRequest>(
    `rechunk-${id}`,
    (_, { arg }) => api.items.rechunk(id, arg)
  )
}

// Search hook
export function useSearch(query: string, sourceId?: string, topK: number = 10) {
  return useSWR<SearchResultsBundle>(
    query ? ["search", query, sourceId, topK] : null,
    () =>
      api.search({
        query,
        source_id: sourceId,
        top_k: topK,
      }),
    {
      revalidateOnFocus: false,
    }
  )
}

// Edges hooks
export function useGraphStatus() {
  return useSWR<GraphStatus>("graph-status", api.graph.status, {
    revalidateOnFocus: false,
  })
}

export function useEdges() {
  return useSWR<Edge[]>("edges", api.edges.list)
}

export function useEdgesForItem(itemId: string | null) {
  return useSWR<Edge[]>(
    itemId ? ["edges", itemId] : null,
    ([, edgeItemId]) => api.edges.listForItem(edgeItemId as string)
  )
}

export function useGraphNeighborhood(
  itemId: string | null,
  depth: number,
  limit: number = 50
) {
  return useSWR<GraphNeighborhood>(
    itemId ? ["graph-neighborhood", itemId, depth, limit] : null,
    ([, neighborhoodItemId, neighborhoodDepth, neighborhoodLimit]) =>
      api.edges.neighborhood(
        neighborhoodItemId as string,
        neighborhoodDepth as number,
        neighborhoodLimit as number
      ),
    {
      revalidateOnFocus: false,
    }
  )
}

export function useCreateEdge() {
  return useSWRMutation<Edge, Error, string, CreateEdgeRequest>(
    "edges",
    (_, { arg }) => api.edges.create(arg)
  )
}

export function useDeleteEdge() {
  return useSWRMutation<void, Error, string, string>(
    "edges",
    (_, { arg }) => api.edges.delete(arg)
  )
}

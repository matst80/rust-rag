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
  DuplicateEdgeGroup,
  ListItemsRequest,
  RechunkRequest,
  LlmRechunkRequest,
  RechunkResponse,
  PagedItems,
  ShareResponse,
  CreateShareRequest,
  Attachment,
  EntriesTreeResponse,
  EntriesPathsResponse,
  SchemaDefinition,
  UpsertSchemaRequest,
  DeleteSchemaResponse,
  UpdateEdgeRequest,
  MapPoint,
  DriveSearchResult,
  HarnessTreeResponse,
  Message,
  SendMessageRequest,
  MessagesResponse,
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

export function useRechunkItem(id: string) {
  return useSWRMutation<RechunkResponse, Error, string, RechunkRequest>(
    `rechunk-${id}`,
    (_, { arg }) => api.items.rechunk(id, arg)
  )
}

export function useReanalyzeItem(id: string) {
  return useSWRMutation<Entry, Error, string[], void>(
    ["item", id],
    () => api.items.reanalyze(id)
  )
}

export function useLlmRechunkItem(id: string) {
  return useSWRMutation<RechunkResponse, Error, string, LlmRechunkRequest>(
    `llm-rechunk-${id}`,
    (_, { arg }) => api.items.llmRechunk(id, arg)
  )
}

// Shares
export function useItemShare(itemId: string | null) {
  return useSWR<ShareResponse | null>(
    itemId ? ["item-share", itemId] : null,
    ([, id]) => api.items.getShare(id as string)
  )
}

export function useCreateShare(itemId: string) {
  return useSWRMutation<ShareResponse, Error, string[], CreateShareRequest | undefined>(
    ["item-share", itemId],
    (_, { arg }) => api.items.createShare(itemId, arg)
  )
}

export function useRevokeShare(itemId: string) {
  return useSWRMutation<void, Error, string[]>(
    ["item-share", itemId],
    () => api.items.revokeShare(itemId)
  )
}

export function usePublicEntry(token: string | null) {
  return useSWR<Entry>(
    token ? ["public-entry", token] : null,
    ([, t]) => api.public.getEntry(t as string)
  )
}

// Attachments
export function useAttachments(itemId: string | null) {
  return useSWR<Attachment[]>(
    itemId ? ["attachments", itemId] : null,
    ([, id]) => api.attachments.list(id as string)
  )
}

export function useUploadAttachment(itemId: string) {
  return useSWRMutation<Attachment, Error, unknown[], File>(
    ["attachments", itemId],
    (_, { arg }) => api.attachments.upload(itemId, arg)
  )
}

export function useAttachUrl(itemId: string) {
  return useSWRMutation<
    Attachment,
    Error,
    unknown[],
    { url: string; filename?: string }
  >(["attachments", itemId], (_, { arg }) =>
    api.attachments.fromUrl(itemId, arg.url, arg.filename)
  )
}

export function useDeleteAttachment(itemId: string) {
  return useSWRMutation<void, Error, unknown[], string>(
    ["attachments", itemId],
    (_, { arg }) => api.attachments.delete(arg)
  )
}

// Wiki tree
export function useEntriesTree(sourceId: string | null, prefix?: string) {
  return useSWR<EntriesTreeResponse>(
    sourceId ? ["entries-tree", sourceId, prefix ?? ""] : null,
    ([, src, p]) => api.tree.get(src as string, (p as string) || undefined)
  )
}

export function useEntriesPaths(sourceId?: string) {
  return useSWR<EntriesPathsResponse>(
    ["entries-paths", sourceId ?? ""],
    ([, src]) => api.tree.paths((src as string) || undefined)
  )
}

// Search hook
export function useSearch(
  query: string,
  sourceId?: string,
  typeName?: string,
  hybrid: boolean = true,
  topK: number = 10,
  rerank: boolean | undefined = undefined,
) {
  return useSWR<SearchResultsBundle>(
    (query || sourceId || typeName)
      ? ["search", query, sourceId, typeName, hybrid, topK, rerank ?? null]
      : null,
    () =>
      api.search({
        query,
        source_id: sourceId,
        type: typeName,
        top_k: topK,
        hybrid,
        ...(rerank !== undefined && { rerank }),
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

export function useMap() {
  return useSWR<MapPoint[]>("api-map", api.map.get, {
    revalidateOnFocus: false,
    revalidateIfStale: false,
  })
}

export function useRebuildMap() {
  return useSWRMutation("/admin/map/rebuild", api.map.rebuild)
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

export function useDuplicateEdges() {
  return useSWR<DuplicateEdgeGroup[]>("duplicate-edges", api.edges.listDuplicates)
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

export function useUpdateEdge(id: string) {
  return useSWRMutation<Edge, Error, any, UpdateEdgeRequest>(
    ["edge", id],
    (_key: any, { arg }: { arg: UpdateEdgeRequest }) => api.edges.update(id, arg)
  )
}

export function useDeleteEdge() {
  return useSWRMutation<void, Error, string, string>(
    "edges",
    (_, { arg }) => api.edges.delete(arg)
  )
}

// Schemas
export function useSchemas() {
  return useSWR<SchemaDefinition[]>("schemas", api.schemas.list)
}

export function useSchema(typeName: string | null) {
  return useSWR<SchemaDefinition>(
    typeName ? ["schema", typeName] : null,
    ([, t]) => api.schemas.get(t as string)
  )
}

export function useUpsertSchema(typeName: string) {
  return useSWRMutation<SchemaDefinition, Error, string[], UpsertSchemaRequest>(
    ["schema", typeName],
    (_, { arg }) => api.schemas.upsert(typeName, arg)
  )
}

export function useDeleteSchema() {
  return useSWRMutation<
    DeleteSchemaResponse,
    Error,
    string,
    { typeName: string; force?: boolean }
  >(
    "schemas",
    (_, { arg }) => api.schemas.delete(arg.typeName, arg.force ?? false)
  )
}
// Integrations
export function useDriveSearch(query: string, mimeType?: string) {
  return useSWR<DriveSearchResult>(
    query ? ["drive-search", query, mimeType] : null,
    () => api.integrations.google.drive.search(query, mimeType),
    { revalidateOnFocus: false }
  )
}

// Harness (Grill) hooks
export function useHarnessTree(sourceId?: string | null) {
  return useSWR<HarnessTreeResponse>(
    ["harness-tree", sourceId ?? null],
    ([, sid]) => api.harness.tree((sid as string | null) ?? undefined)
  )
}

// Messages & Comments hooks
export function useChannelMessages(channel: string | null) {
  return useSWR<MessagesResponse>(
    channel ? ["channel-messages", channel] : null,
    () => api.messages.list({ channel: channel!, limit: 100 }),
    { refreshInterval: 5000 }
  )
}

export function useSendMessage() {
  return useSWRMutation<Message, Error, string, SendMessageRequest>(
    "channel-messages",
    (_, { arg }) => api.messages.send(arg)
  )
}

export function useDeleteMessage() {
  return useSWRMutation<void, Error, string, string>(
    "channel-messages",
    (_, { arg }) => api.messages.delete(arg)
  )
}

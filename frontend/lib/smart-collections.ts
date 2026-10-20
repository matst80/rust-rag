"use client"
import type { SortOrder } from "@/lib/api"
export interface SmartCollection { id: string; inbox?: boolean; name: string; category: string | null; path: string | null; type: string | null; metadata: Record<string, string[]>; search: string; sort: SortOrder }
const KEY = "rag:smart-collections:v1"
export function getCollections(): SmartCollection[] { try { const value = JSON.parse(localStorage.getItem(KEY) ?? "[]"); return Array.isArray(value) ? value.filter(c => c && typeof c.id === "string" && typeof c.name === "string" && c.metadata && typeof c.metadata === "object") : [] } catch { return [] } }
export function saveCollections(value: SmartCollection[]) { localStorage.setItem(KEY, JSON.stringify(value)); window.dispatchEvent(new Event("collections-changed")) }

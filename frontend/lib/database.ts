"use client"

import type { Entry, SchemaDefinition } from "@/lib/api/types"

export type DbPropKind = "enum" | "date" | "boolean" | "number" | "array" | "string" | "other"

export interface DbProperty {
  key: string
  kind: DbPropKind
  title: string
  enumValues?: string[]
  required?: boolean
}

function prettyKey(key: string): string {
  return key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())
}

/** Flatten a JSON-schema object into Notion-like property descriptors. */
export function schemaProperties(schema: SchemaDefinition): DbProperty[] {
  const raw = schema.json_schema as Record<string, unknown>
  const props = (raw?.properties as Record<string, Record<string, unknown>> | undefined) ?? {}
  const required = new Set((raw?.required as string[] | undefined) ?? [])
  return Object.entries(props).map(([key, def]) => {
    const type = def?.type as string | undefined
    const format = def?.format as string | undefined
    const enumValues = Array.isArray(def?.enum)
      ? (def.enum as unknown[]).map(String)
      : undefined
    let kind: DbPropKind = "other"
    if (enumValues && enumValues.length > 0) kind = "enum"
    else if (format === "date" || format === "date-time" || /date|due|at$|_on$/i.test(key)) {
      kind = type === "string" || !type ? "date" : kind
      if (kind === "other") kind = "date"
    }
    else if (type === "boolean") kind = "boolean"
    else if (type === "number" || type === "integer") kind = "number"
    else if (type === "array") kind = "array"
    else if (type === "string" || !type) kind = "string"
    return {
      key,
      kind,
      title: (def?.title as string | undefined) ?? prettyKey(key),
      enumValues,
      required: required.has(key),
    }
  })
}

export function groupByCandidates(props: DbProperty[]): DbProperty[] {
  return props.filter((p) => p.kind === "enum")
}

export function dateCandidates(props: DbProperty[]): DbProperty[] {
  return props.filter((p) => p.kind === "date")
}

export function defaultGroupBy(props: DbProperty[]): DbProperty | null {
  const preferred = ["status", "state", "stage", "phase", "priority"]
  for (const name of preferred) {
    const hit = props.find((p) => p.key === name && p.kind === "enum")
    if (hit) return hit
  }
  return props.find((p) => p.kind === "enum") ?? null
}

export function defaultDateProp(props: DbProperty[]): DbProperty | null {
  const preferred = ["due", "due_date", "date", "decided_at", "created", "deadline", "scheduled"]
  for (const name of preferred) {
    const hit = props.find((p) => p.key === name && p.kind === "date")
    if (hit) return hit
  }
  return props.find((p) => p.kind === "date") ?? null
}

export function titleProp(props: DbProperty[]): DbProperty | null {
  return (
    props.find((p) => p.key === "title") ??
    props.find((p) => p.key === "name") ??
    props.find((p) => p.kind === "string") ??
    null
  )
}

export function entryTitle(entry: Entry, fallbackProp?: DbProperty | null): string {
  const data = (entry.data ?? {}) as Record<string, unknown>
  for (const key of ["title", "name", fallbackProp?.key ?? ""]) {
    if (!key) continue
    const v = data[key]
    if (typeof v === "string" && v.trim()) return v
  }
  const metaTitle = entry.metadata?.title
  if (typeof metaTitle === "string" && metaTitle.trim()) return metaTitle
  const firstLine = entry.text.split("\n")[0]?.replace(/^#+\s*/, "").trim()
  return firstLine || entry.id
}

export function propValue(entry: Entry, key: string): unknown {
  return (entry.data as Record<string, unknown> | null)?.[key]
}

export function formatPropValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—"
  if (typeof value === "boolean") return value ? "Yes" : "No"
  if (Array.isArray(value)) return value.length > 0 ? value.map(String).join(", ") : "—"
  if (typeof value === "object") return "…"
  return String(value)
}

/** Parse a `YYYY-MM-DD` (or datetime) value into a day key, or null. */
export function dateKeyOf(value: unknown): string | null {
  if (typeof value !== "string" || !value) return null
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(value)
  return m ? `${m[1]}-${m[2]}-${m[3]}` : null
}

export type DatabaseViewKind = "table" | "board" | "calendar"

export function viewStorageKey(typeName: string): string {
  return `database:${typeName}:view`
}

export function groupByStorageKey(typeName: string): string {
  return `database:${typeName}:groupBy`
}

export function datePropStorageKey(typeName: string): string {
  return `database:${typeName}:dateProp`
}

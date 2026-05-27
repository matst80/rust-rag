import "server-only"

import { getAuthConfig } from "@/lib/auth/config"
import { headers } from "next/headers"
import type { CmsTreeResponse } from "@/lib/api/types"

export async function fetchCmsTree(id: string) {
  const cfg = getAuthConfig()
  const requestHeaders = await headers()
  const headers: HeadersInit = {
    Accept: "application/json",
  }

  const cookie = requestHeaders.get("cookie")
  if (cookie) {
    headers["cookie"] = cookie
  }
  if (cfg.backendApiKey) {
    headers["x-api-key"] = cfg.backendApiKey
  }

  const response = await fetch(
    `${cfg.appBaseUrl}/api/cms/tree/${encodeURIComponent(id)}`,
    {
      headers,
      cache: "no-store",
    },
  )

  if (!response.ok) {
    const detail = await response.text()
    throw new Error(detail || `${response.status} ${response.statusText}`)
  }

  const payload = (await response.json()) as CmsTreeResponse
  return payload.tree
}

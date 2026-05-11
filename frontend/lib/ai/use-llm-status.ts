"use client"

import { useEffect, useState } from "react"
import { getLlmClient, type LlmStatus } from "./llm-client"

export function useLlmStatus(): LlmStatus {
  const [status, setStatus] = useState<LlmStatus>(() => getLlmClient().status)
  useEffect(() => {
    const client = getLlmClient()
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<LlmStatus>).detail
      setStatus(detail)
    }
    client.addEventListener("status", handler)
    setStatus(client.status)
    return () => client.removeEventListener("status", handler)
  }, [])
  return status
}

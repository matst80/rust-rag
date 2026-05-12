import { useEffect, useState } from "react"
import { getLlmClient, type LlmStatus, type ProfileKey } from "./client"

export function useLlmStatus(profile: ProfileKey = "text"): LlmStatus {
  const [status, setStatus] = useState<LlmStatus>(() => getLlmClient(profile).status)
  useEffect(() => {
    const client = getLlmClient(profile)
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<LlmStatus>).detail
      setStatus(detail)
    }
    client.addEventListener("status", handler)
    setStatus(client.status)
    return () => client.removeEventListener("status", handler)
  }, [profile])
  return status
}

"use client"

import { useEffect, useState } from "react"

function read<T>(key: string, initial: T): T {
  if (typeof window === "undefined") return initial
  try {
    const raw = window.sessionStorage.getItem(key)
    return raw === null ? initial : (JSON.parse(raw) as T)
  } catch {
    return initial
  }
}

export function useSessionState<T>(
  key: string,
  initial: T
): [T, (v: T | ((p: T) => T)) => void] {
  const [value, setValue] = useState<T>(() => read(key, initial))

  useEffect(() => {
    try {
      window.sessionStorage.setItem(key, JSON.stringify(value))
    } catch {
      // ignore
    }
  }, [key, value])

  return [value, setValue]
}

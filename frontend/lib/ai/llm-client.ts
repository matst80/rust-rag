"use client"

// Client-side LLM inference via @mediapipe/tasks-genai (LiteRT-powered WebGPU
// runtime). Singleton + EventTarget so multiple UI components share one model.

import type { LlmInference as LlmInferenceType } from "@mediapipe/tasks-genai"

const DEFAULT_MODEL_URL =
  process.env.NEXT_PUBLIC_LLM_MODEL_URL ??
  "https://huggingface.co/litert-community/gemma-4-E2B-it-litert-lm/resolve/main/gemma-4-E2B-it-web.task"

const WASM_BASE = "https://cdn.jsdelivr.net/npm/@mediapipe/tasks-genai/wasm"

export type LlmStatus =
  | { kind: "idle" }
  | { kind: "loading"; loaded: number; total: number | null; source: "network" | "cache" }
  | { kind: "ready" }
  | { kind: "generating" }
  | { kind: "error"; message: string }

class LlmClient extends EventTarget {
  private llm: LlmInferenceType | null = null
  private loadPromise: Promise<LlmInferenceType> | null = null
  private _status: LlmStatus = { kind: "idle" }
  // Serialize generateResponse calls — MediaPipe only allows one at a time.
  private inflight: Promise<unknown> = Promise.resolve()

  get status(): LlmStatus {
    return this._status
  }

  private setStatus(s: LlmStatus) {
    this._status = s
    this.dispatchEvent(new CustomEvent("status", { detail: s }))
  }

  isSupported(): boolean {
    if (typeof navigator === "undefined") return false
    return typeof (navigator as unknown as { gpu?: unknown }).gpu !== "undefined"
  }

  private async fetchModelBuffer(modelUrl: string): Promise<Uint8Array> {
    const cacheName = "litert-models-v1"
    let cache: Cache | null = null
    try {
      cache = await caches.open(cacheName)
    } catch (err) {
      console.warn("[llm] caches.open failed", err)
    }

    if (cache) {
      try {
        // ignoreVary because HF sometimes sends Vary: * which otherwise misses.
        const cached = await cache.match(modelUrl, { ignoreVary: true })
        if (cached) {
          console.info("[llm] model cache HIT", modelUrl)
          return this.streamBody(cached, "cache")
        }
        console.info("[llm] model cache MISS", modelUrl)
      } catch (err) {
        console.warn("[llm] cache.match threw", err)
      }
    }

    // Strip headers that block caching (Vary: *, Set-Cookie). We rebuild the
    // Response with the same body but only the headers we care about.
    const response = await fetch(modelUrl)
    if (!response.ok) {
      throw new Error(
        `Model fetch failed: ${response.status} ${response.statusText}`
      )
    }

    if (cache) {
      try {
        const cloneForStore = response.clone()
        // Re-wrap to drop Vary and other troublesome headers before cache.put.
        const sanitizedHeaders = new Headers()
        const ct = cloneForStore.headers.get("content-type")
        const cl = cloneForStore.headers.get("content-length")
        if (ct) sanitizedHeaders.set("content-type", ct)
        if (cl) sanitizedHeaders.set("content-length", cl)
        const sanitized = new Response(cloneForStore.body, {
          status: cloneForStore.status,
          statusText: cloneForStore.statusText,
          headers: sanitizedHeaders,
        })
        await cache.put(modelUrl, sanitized)
        console.info("[llm] model cached to CacheStorage", modelUrl)
      } catch (err) {
        console.warn(
          "[llm] cache.put failed; model will re-download next time",
          err
        )
      }
    }
    return this.streamBody(response, "network")
  }

  private async streamBody(
    response: Response,
    source: "cache" | "network"
  ): Promise<Uint8Array> {
    const totalHeader = response.headers.get("content-length")
    const total = totalHeader ? parseInt(totalHeader, 10) : null

    const reader = response.body?.getReader()
    if (!reader) throw new Error("ReadableStream unsupported")

    const chunks: Uint8Array[] = []
    let loaded = 0
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      if (value) {
        chunks.push(value)
        loaded += value.byteLength
        this.setStatus({ kind: "loading", loaded, total, source })
      }
    }
    const buffer = new Uint8Array(loaded)
    let off = 0
    for (const c of chunks) {
      buffer.set(c, off)
      off += c.byteLength
    }
    return buffer
  }

  async load(modelUrl: string = DEFAULT_MODEL_URL): Promise<LlmInferenceType> {
    if (this.llm) return this.llm
    if (this.loadPromise) return this.loadPromise

    this.loadPromise = (async () => {
      try {
        if (!this.isSupported()) {
          throw new Error("WebGPU is not available in this browser.")
        }

        this.setStatus({ kind: "loading", loaded: 0, total: null, source: "network" })
        const buffer = await this.fetchModelBuffer(modelUrl)

        // Dynamic import keeps MediaPipe out of the initial bundle.
        const { FilesetResolver, LlmInference } = await import(
          "@mediapipe/tasks-genai"
        )
        const fileset = await FilesetResolver.forGenAiTasks(WASM_BASE)
        const llm = await LlmInference.createFromOptions(fileset, {
          baseOptions: { modelAssetBuffer: buffer },
          maxTokens: 2048,
          topK: 40,
          temperature: 0.6,
          randomSeed: 1,
        })

        this.llm = llm
        this.setStatus({ kind: "ready" })
        return llm
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        this.setStatus({ kind: "error", message })
        this.loadPromise = null
        throw err
      }
    })()

    return this.loadPromise
  }

  /** Stream a response. `onToken` receives the cumulative text on every chunk. */
  async generate(
    prompt: string,
    onToken: (partial: string, done: boolean) => void,
    signal?: AbortSignal
  ): Promise<string> {
    const llm = await this.load()
    // Serialize through inflight queue.
    const previous = this.inflight
    let resolveQueue: () => void = () => {}
    this.inflight = new Promise<void>((r) => { resolveQueue = r })
    await previous

    try {
      if (signal?.aborted) throw new Error("aborted")
      this.setStatus({ kind: "generating" })
      let accumulated = ""
      const finalText = await llm.generateResponse(prompt, (partial, done) => {
        accumulated += partial
        onToken(accumulated, done)
      })
      this.setStatus({ kind: "ready" })
      return finalText ?? accumulated
    } catch (err) {
      this.setStatus({ kind: "ready" })
      throw err
    } finally {
      resolveQueue()
    }
  }
}

let _client: LlmClient | null = null
export function getLlmClient(): LlmClient {
  if (!_client) _client = new LlmClient()
  return _client
}

export function formatLoadProgress(s: LlmStatus): string {
  if (s.kind !== "loading") return ""
  const mb = (n: number) => (n / 1024 / 1024).toFixed(0)
  const tag = s.source === "cache" ? "cache" : "downloading"
  if (s.total) {
    const pct = ((s.loaded / s.total) * 100).toFixed(1)
    return `${tag} · ${mb(s.loaded)} / ${mb(s.total)} MB · ${pct}%`
  }
  return `${tag} · ${mb(s.loaded)} MB`
}

/** Drop the cached model — next load will re-download. */
export async function clearModelCache(): Promise<void> {
  try {
    await caches.delete("litert-models-v1")
  } catch {
    // ignore
  }
}

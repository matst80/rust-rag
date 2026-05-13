// Client-side LLM inference via @mediapipe/tasks-genai (LiteRT-powered WebGPU).
// One instance per model profile (text vs vision). Cached in CacheStorage.

import type {
  LlmInference as LlmInferenceType,
  Prompt,
} from "@mediapipe/tasks-genai"

let WASM_BASE = "https://cdn.jsdelivr.net/npm/@mediapipe/tasks-genai/wasm"

export function setWasmBase(base: string): void {
  WASM_BASE = base
}
const CACHE_NAME = "litert-models-v1"

export interface ModelProfile {
  url: string
  /** Passed straight to LlmInference.createFromOptions. */
  options: {
    maxTokens?: number
    topK?: number
    temperature?: number
    randomSeed?: number
    maxNumImages?: number
    maxNumVideos?: number
    supportAudio?: boolean
  }
}

const DEFAULT_TEXT_URL =
  "https://huggingface.co/litert-community/gemma-4-E4B-it-litert-lm/resolve/main/gemma-4-E4B-it-web.task"
// Gemma 3n is gated on HF (401 without token). Try Gemma 4 web.task with
// maxNumImages set — if MediaPipe rejects the modality, the error will be
// explicit and we can swap back. Cheap experiment.
const DEFAULT_VISION_URL =
  "https://huggingface.co/litert-community/gemma-4-E4B-it-litert-lm/resolve/main/gemma-4-E4B-it-web.task"

/** Resolve a model URL by checking common env / global override slots. */
function resolveUrl(globalKey: string, fallback: string): string {
  const proc = (globalThis as unknown as {
    process?: { env?: Record<string, string | undefined> }
  }).process
  const env = proc?.env
  if (env) {
    if (globalKey === "text" && env.NEXT_PUBLIC_LLM_MODEL_URL)
      return env.NEXT_PUBLIC_LLM_MODEL_URL
    if (globalKey === "vision" && env.NEXT_PUBLIC_LLM_VISION_MODEL_URL)
      return env.NEXT_PUBLIC_LLM_VISION_MODEL_URL
  }
  const g = globalThis as unknown as Record<string, string | undefined>
  const key = globalKey === "vision" ? "RUST_RAG_LLM_VISION_URL" : "RUST_RAG_LLM_URL"
  if (g[key]) return g[key] as string
  return fallback
}

export const MODEL_PROFILES = {
  text: (): ModelProfile => ({
    url: resolveUrl("text", DEFAULT_TEXT_URL),
    options: { maxTokens: 2048, topK: 40, temperature: 0.6, randomSeed: 1 },
  }),
  vision: (): ModelProfile => ({
    url: resolveUrl("vision", DEFAULT_VISION_URL),
    options: {
      maxTokens: 1024,
      topK: 40,
      temperature: 0.4,
      randomSeed: 1,
      maxNumImages: 5,
      // Removed maxNumVideos to ensure vision-only graph can initialize if video is unsupported
      supportAudio: true,
    },
  }),
}

export type ProfileKey = keyof typeof MODEL_PROFILES

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
  private inflight: Promise<unknown> = Promise.resolve()

  constructor(public readonly profile: ProfileKey) {
    super()
  }

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

  private async fetchModelBuffer(modelUrl: string): Promise<Blob> {
    let cache: Cache | null = null
    try {
      cache = await caches.open(CACHE_NAME)
    } catch (err) {
      console.warn("[llm] caches.open failed", err)
    }

    if (cache) {
      try {
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

    const response = await fetch(modelUrl)
    if (!response.ok) {
      throw new Error(
        `Model fetch failed: ${response.status} ${response.statusText}`
      )
    }

    if (cache) {
      try {
        const cloneForStore = response.clone()
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
  ): Promise<Blob> {
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
    return new Blob(chunks as BlobPart[])
  }

  async load(): Promise<LlmInferenceType> {
    if (this.llm) return this.llm
    if (this.loadPromise) return this.loadPromise

    const profile = MODEL_PROFILES[this.profile]()

    this.loadPromise = (async () => {
      try {
        if (!this.isSupported()) {
          throw new Error("WebGPU is not available in this browser.")
        }

        this.setStatus({ kind: "loading", loaded: 0, total: null, source: "network" })
        const blob = await this.fetchModelBuffer(profile.url)
        const blobUrl = URL.createObjectURL(blob)

        const { FilesetResolver, LlmInference } = await import(
          "@mediapipe/tasks-genai"
        )
        const fileset = await FilesetResolver.forGenAiTasks(WASM_BASE)
        const llm = await LlmInference.createFromOptions(fileset, {
          baseOptions: { modelAssetPath: blobUrl },
          ...profile.options,
        })

        // We can revoke the URL after creation, MediaPipe has loaded it
        URL.revokeObjectURL(blobUrl)

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

  /** Stream a response. `onToken` receives cumulative text on every chunk. */
  async generate(
    prompt: Prompt,
    onToken: (partial: string, done: boolean) => void,
    signal?: AbortSignal
  ): Promise<string> {
    const llm = await this.load()
    const previous = this.inflight
    let resolveQueue: () => void = () => {}
    this.inflight = new Promise<void>((r) => { resolveQueue = r })
    await previous

    try {
      if (signal?.aborted) throw new Error("aborted")
      this.setStatus({ kind: "generating" })

      let finalPrompt = prompt
      if (typeof prompt === "string" && !prompt.includes("<|turn>")) {
        // Apply Gemma 4 instruction template for raw strings
        finalPrompt = `<|turn>user\n${prompt}<turn|>\n<|turn>model\n`
      }

      let accumulated = ""
      const finalText = await llm.generateResponse(finalPrompt, (partial, done) => {
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

const _clients = new Map<ProfileKey, LlmClient>()
export function getLlmClient(profile: ProfileKey = "text"): LlmClient {
  let c = _clients.get(profile)
  if (!c) {
    c = new LlmClient(profile)
    _clients.set(profile, c)
  }
  return c
}

export type { LlmClient }

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

export async function clearModelCache(): Promise<void> {
  try {
    await caches.delete(CACHE_NAME)
  } catch {
    // ignore
  }
}

export async function requestPersistentStorage(): Promise<boolean> {
  try {
    if (
      typeof navigator !== "undefined" &&
      navigator.storage &&
      "persist" in navigator.storage
    ) {
      return await navigator.storage.persist()
    }
  } catch {
    // ignore
  }
  return false
}

export function isWebGpuAvailable(): boolean {
  if (typeof navigator === "undefined") return false
  return Boolean((navigator as unknown as { gpu?: unknown }).gpu)
}

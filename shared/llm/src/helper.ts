import {
  Gemma4ForConditionalGeneration,
  AutoProcessor,
  TextStreamer,
  load_image,
  env,
} from "@huggingface/transformers"

// Configure WASM paths to use your local files in public/wasm/
if (env.backends.onnx.wasm) {
  env.backends.onnx.wasm.wasmPaths = "/wasm/"
}

// Remote fetching enabled - using the public Gemma 4 model ID
env.allowRemoteModels = true 
env.allowLocalModels = false

export interface LlmHelperStatus {
  kind: "idle" | "loading" | "ready" | "generating" | "error"
  progress?: number
  message?: string
}

export interface GenerateOptions {
  prompt: string
  images?: (HTMLImageElement | ImageBitmap | HTMLCanvasElement | Blob | string)[]
  audios?: Blob[]
  maxTokens?: number
  temperature?: number
  onToken?: (token: string, done: boolean) => void
  signal?: AbortSignal
}

export class LlmHelper extends EventTarget {
  private model: any = null
  private processor: any = null
  private _status: LlmHelperStatus = { kind: "idle" }

  constructor(public readonly modelId: string = "onnx-community/gemma-4-E4B-it-ONNX") {
    super()
  }

  get status() {
    return this._status
  }

  private setStatus(status: LlmHelperStatus) {
    this._status = status
    this.dispatchEvent(new CustomEvent("status", { detail: status }))
  }

  async load() {
    if (this.model && this.processor) return

    this.setStatus({ kind: "loading", progress: 0 })

    try {
      // Use the specific Gemma4 class as in the example
      const [model, processor] = await Promise.all([
        Gemma4ForConditionalGeneration.from_pretrained(this.modelId, {
          dtype: "q4f16",
          device: "webgpu",
          progress_callback: (p: any) => {
            if (p.status === "progress" && p.file != null) {
              this.setStatus({ kind: "loading", progress: p.progress })
            }
          },
        }),
        AutoProcessor.from_pretrained(this.modelId),
      ])

      this.model = model
      this.processor = processor
      this.setStatus({ kind: "ready" })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      this.setStatus({ kind: "error", message })
      throw err
    }
  }

  async generate(opts: GenerateOptions): Promise<string> {
    await this.load()
    this.setStatus({ kind: "generating" })

    try {
      // 1. Prepare Multimodal Prompt
      let fullPrompt = `<|turn>user\n${opts.prompt}`
      if (opts.images?.length) {
        fullPrompt += "\n" + opts.images.map(() => "<|image|>").join("\n")
      }
      if (opts.audios?.length) {
        fullPrompt += "\n" + opts.audios.map(() => "<|audio|>").join("\n")
      }
      fullPrompt += "<turn|>\n<|turn>model\n"

      // 2. Process Inputs
      const images = opts.images
        ? await Promise.all(opts.images.map((img) => load_image(img as any)))
        : null

      const audios = opts.audios
        ? await Promise.all(opts.audios.map(async (blob) => {
            const buffer = await blob.arrayBuffer()
            return new Float32Array(buffer)
          }))
        : null

      const inputs = await this.processor(
        fullPrompt,
        images,
        audios,
        { add_special_tokens: false }
      )

      // 3. Setup Streaming
      let accumulated = ""
      const streamer = new TextStreamer(this.processor.tokenizer, {
        skip_prompt: true,
        callback_function: (token: string) => {
          accumulated += token
          opts.onToken?.(accumulated, false)
        },
      })

      // 4. Run Inference
      await this.model.generate({
        ...inputs,
        max_new_tokens: opts.maxTokens ?? 1024,
        do_sample: false, // Default to greedy as in example
        streamer,
      })

      opts.onToken?.(accumulated, true)
      this.setStatus({ kind: "ready" })
      return accumulated
    } catch (err) {
      this.setStatus({ kind: "ready" })
      throw err
    }
  }
}

// Global instance for simple usage
let _defaultHelper: LlmHelper | null = null
export function getLlmHelper(modelId?: string): LlmHelper {
  if (!_defaultHelper) {
    _defaultHelper = new LlmHelper(modelId)
  }
  return _defaultHelper
}

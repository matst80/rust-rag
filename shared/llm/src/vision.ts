import { getLlmHelper } from "./helper"

export interface CaptionOptions {
  prompt: string
  onToken?: (partial: string, done: boolean) => void
  signal?: AbortSignal
}

/**
 * Caption / OCR an image using the multimodal helper.
 * Uses Transformers.js v3 + WebGPU.
 */
export async function captionImage(
  image: HTMLImageElement | ImageBitmap | HTMLCanvasElement | Blob,
  opts: CaptionOptions
): Promise<string> {
  const helper = getLlmHelper()
  
  return helper.generate({
    prompt: opts.prompt,
    images: [image],
    onToken: opts.onToken,
    signal: opts.signal,
  })
}

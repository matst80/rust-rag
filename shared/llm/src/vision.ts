import { getLlmClient } from "./client"

export interface CaptionOptions {
  prompt: string
  onToken?: (partial: string, done: boolean) => void
  signal?: AbortSignal
}

/**
 * Caption / OCR an image using the vision model profile.
 * Accepts anything Canvas can rasterize plus raw Blobs (auto-decoded).
 */
export async function captionImage(
  image: HTMLImageElement | ImageBitmap | HTMLCanvasElement | Blob,
  opts: CaptionOptions
): Promise<string> {
  const client = getLlmClient("vision")
  const bitmap =
    image instanceof Blob ? await createImageBitmap(image) : image
  const prompt = [{ imageSource: bitmap }, opts.prompt] as const
  let last = ""
  return client.generate(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    prompt as any,
    (partial, done) => {
      last = partial
      opts.onToken?.(partial, done)
    },
    opts.signal
  ).then((final) => final ?? last)
}

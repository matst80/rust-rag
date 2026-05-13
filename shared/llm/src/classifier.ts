import { FilesetResolver, ImageClassifier, type ImageClassifierResult } from "@mediapipe/tasks-vision"

let visionWasmBase = "https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision/wasm"

export function setVisionWasmBase(base: string): void {
  visionWasmBase = base
}

const DEFAULT_CLASSIFIER_MODEL =
  "https://storage.googleapis.com/mediapipe-models/image_classifier/efficientnet_lite0/float32/1/efficientnet_lite0.tflite"

let classifier: ImageClassifier | null = null

export interface ClassifyOptions {
  modelUrl?: string
  maxResults?: number
  scoreThreshold?: number
}

export async function classifyImage(
  image: HTMLImageElement | ImageBitmap | HTMLCanvasElement | Blob,
  opts: ClassifyOptions = {}
): Promise<ImageClassifierResult> {
  if (!classifier) {
    const vision = await FilesetResolver.forVisionTasks(visionWasmBase)
    classifier = await ImageClassifier.createFromOptions(vision, {
      baseOptions: {
        modelAssetPath: opts.modelUrl || DEFAULT_CLASSIFIER_MODEL,
        delegate: "GPU",
      },
      runningMode: "IMAGE",
      maxResults: opts.maxResults ?? 3,
      scoreThreshold: opts.scoreThreshold ?? 0.1,
    })
  }

  const input = image instanceof Blob ? await createImageBitmap(image) : image
  return classifier.classify(input)
}

export function formatClassificationResult(result: ImageClassifierResult): string {
  if (!result.classifications || result.classifications.length === 0) return ""
  const categories = result.classifications[0].categories
  return categories
    .map((c) => `${c.categoryName} (${(c.score * 100).toFixed(1)}%)`)
    .join(", ")
}

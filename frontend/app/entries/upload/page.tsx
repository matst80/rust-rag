import { AppHeader } from "@/components/app-header"
import { ImageUpload } from "@/components/entries/image-upload"

export const metadata = {
  title: "Upload Image | RAG Memory & Knowledge",
  description: "Upload an image to extract and index its content",
}

export default function UploadPage() {
  return (
    <>
      <AppHeader />
      <main>
        <ImageUpload />
      </main>
    </>
  )
}

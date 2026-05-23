import { redirect } from "next/navigation"

export default function UploadPage() {
  redirect("/entries/new?tab=image")
}

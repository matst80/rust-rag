import { AppHeader } from "@/components/app-header"
import { AssistedQueryView } from "@/components/query/assisted-query"

export default function AssistedPage() {
  return (
    <>
      <AppHeader />
      <main className="container py-6">
        <AssistedQueryView />
      </main>
    </>
  )
}

import { Suspense } from "react"
import { AppHeader } from "@/components/app-header"
import { SearchPage } from "@/components/search/search-page"

export default function AssistedPage() {
  return (
    <>
      <AppHeader />
      <main>
        <Suspense fallback={null}>
          <SearchPage defaultAssisted={true} />
        </Suspense>
      </main>
    </>
  )
}

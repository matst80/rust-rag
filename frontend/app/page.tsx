import { Suspense } from "react"
import { AppHeader } from "@/components/app-header"
import { SearchPage } from "@/components/search/search-page"

export default function Home() {
  return (
    <>
      <AppHeader />
      <main>
        <Suspense fallback={null}>
          <SearchPage />
        </Suspense>
      </main>
    </>
  )
}

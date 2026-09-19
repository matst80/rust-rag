import { Suspense } from "react"
import { SearchPage } from "@/components/search/search-page"

export default function AssistedPage() {
  return (
    <>
        <Suspense fallback={null}>
          <SearchPage defaultAssisted={true} />
        </Suspense>
    </>
  )
}

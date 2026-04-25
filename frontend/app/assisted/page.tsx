import { AppHeader } from "@/components/app-header"
import { SearchPage } from "@/components/search/search-page"

export default function AssistedPage() {
  return (
    <>
      <AppHeader />
      <main>
        <SearchPage defaultAssisted={true} />
      </main>
    </>
  )
}

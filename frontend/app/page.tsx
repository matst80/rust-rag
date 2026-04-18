import { AppHeader } from "@/components/app-header"
import { SearchPage } from "@/components/search/search-page"

export default function Home() {
  return (
    <>
      <AppHeader />
      <main>
        <SearchPage />
      </main>
    </>
  )
}

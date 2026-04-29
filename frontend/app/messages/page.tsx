import { AppHeader } from "@/components/app-header"
import { MessagesInterface } from "@/components/messages/messages-interface"

export default function MessagesPage() {
  return (
    <>
      <AppHeader />
      <main>
        <MessagesInterface />
      </main>
    </>
  )
}

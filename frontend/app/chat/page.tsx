import { AppHeader } from "@/components/app-header"
import { ChatInterface } from "@/components/chat/chat-interface"

export default function ChatPage() {
  return (
    <>
      <AppHeader />
      <main className="container py-6">
        <ChatInterface />
      </main>
    </>
  )
}

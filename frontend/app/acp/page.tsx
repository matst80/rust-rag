import { AgentChat } from "@/components/acp/agent-chat"

export const dynamic = "force-dynamic"

export default function AcpPage() {
	return (
		<main className="container mx-auto p-4 h-[calc(100vh-4rem)]">
			<AgentChat />
		</main>
	)
}

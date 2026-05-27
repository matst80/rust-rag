import { AppHeader } from "@/components/app-header"
import { AgentChat } from "@/components/acp/agent-chat"

export const dynamic = "force-dynamic"

export default function AcpPage() {
	return (
		<div className="flex flex-col h-dvh overflow-hidden">
			<AppHeader />
			<main className="flex-1 min-h-0">
				<AgentChat />
			</main>
		</div>
	)
}

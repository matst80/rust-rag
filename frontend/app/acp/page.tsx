import { AppHeader } from "@/components/app-header"
import { AgentChat } from "@/components/acp/agent-chat"

export const dynamic = "force-dynamic"

export default function AcpPage() {
	return (
		<>
			<AppHeader />
			<main>
				<AgentChat />
			</main>
		</>
	)
}

import { AgentChat } from "@/components/acp/agent-chat"

export const dynamic = "force-dynamic"

export default function AcpPage() {
	return (
		<div className="flex h-full min-h-0 flex-col overflow-hidden">
			<AgentChat />
		</div>
	)
}

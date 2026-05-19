import { redirect } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { GoogleIntegrationCard } from "@/components/integrations/google-card"
import { NotificationsCard } from "@/components/integrations/notifications-card"
import { readSessionFromCookies } from "@/lib/auth/session"

export default async function IntegrationsPage() {
	const session = await readSessionFromCookies()
	if (!session) {
		redirect(`/auth/login?returnTo=${encodeURIComponent("/settings/integrations")}`)
	}

	return (
		<>
			<AppHeader />
			<main className="mx-auto flex max-w-3xl flex-col gap-6 px-4 py-12">
				<div className="space-y-2">
					<h1 className="text-2xl font-semibold">Integrations</h1>
					<p className="text-sm text-muted-foreground">
						Connect external accounts so agents can pull mail, calendar, and
						drive documents on your behalf. Tokens are stored encrypted at
						rest and scoped to{" "}
						<span className="font-mono">
							{session.email ?? session.preferred_username ?? session.sub}
						</span>
						.
					</p>
				</div>
				<GoogleIntegrationCard />
				<NotificationsCard />
			</main>
		</>
	)
}

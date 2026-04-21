import { redirect } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { TokensList } from "@/components/auth/tokens-list"
import { readSessionFromCookies } from "@/lib/auth/session"

export default async function TokensPage() {
	const session = await readSessionFromCookies()
	if (!session) {
		redirect(`/auth/login?returnTo=${encodeURIComponent("/auth/tokens")}`)
	}

	return (
		<>
			<AppHeader />
			<main className="mx-auto flex max-w-3xl flex-col gap-6 px-4 py-12">
				<div className="space-y-2">
					<h1 className="text-2xl font-semibold">MCP tokens</h1>
					<p className="text-sm text-muted-foreground">
						Tokens bound to{" "}
						<span className="font-mono">
							{session.email ?? session.preferred_username ?? session.sub}
						</span>
						. Revoke any token the MCP client no longer needs.
					</p>
				</div>
				<TokensList />
			</main>
		</>
	)
}

import { redirect } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { DeviceApproveForm } from "@/components/auth/device-approve-form"
import { readSessionFromCookies } from "@/lib/auth/session"

interface PageProps {
	searchParams: Promise<{ user_code?: string }>
}

export default async function DeviceApprovePage({ searchParams }: PageProps) {
	const session = await readSessionFromCookies()
	const { user_code: userCode } = await searchParams

	if (!session) {
		const returnTo = userCode
			? `/auth/device?user_code=${encodeURIComponent(userCode)}`
			: "/auth/device"
		redirect(`/auth/login?returnTo=${encodeURIComponent(returnTo)}`)
	}

	return (
		<>
			<AppHeader />
			<main className="mx-auto flex max-w-xl flex-col gap-6 px-4 py-12">
				<div className="space-y-2">
					<h1 className="text-2xl font-semibold">Approve MCP device</h1>
					<p className="text-sm text-muted-foreground">
						Signed in as{" "}
						<span className="font-mono">
							{session.email ?? session.preferred_username ?? session.sub}
						</span>
						. Enter the <strong>user code</strong> shown by the MCP client to mint a
						bearer token bound to your account.
					</p>
				</div>
				<DeviceApproveForm initialUserCode={userCode ?? ""} />
			</main>
		</>
	)
}

"use client"

import useSWR, { mutate } from "swr"
import { useState } from "react"
import { Button } from "@/components/ui/button"

interface StatusResponse {
	connected: boolean
	provider: string
	account_email: string | null
	scopes: string[]
	expires_at: number | null
	updated_at: number | null
}

const STATUS_URL = "/api/integrations/google/status"
const DISCONNECT_URL = "/api/integrations/google/disconnect"
const START_URL = "/api/integrations/google/start?return_to=/settings/integrations"

async function fetcher(url: string): Promise<StatusResponse> {
	const response = await fetch(url, { credentials: "same-origin" })
	if (!response.ok) {
		throw new Error(`failed to load status (${response.status})`)
	}
	return response.json()
}

function formatDate(ms: number | null | undefined): string {
	if (!ms) return "—"
	return new Date(ms).toLocaleString()
}

export function GoogleIntegrationCard() {
	const { data, error, isLoading } = useSWR(STATUS_URL, fetcher)
	const [busy, setBusy] = useState(false)

	async function disconnect() {
		setBusy(true)
		try {
			const response = await fetch(DISCONNECT_URL, {
				method: "POST",
				credentials: "same-origin",
			})
			if (!response.ok) {
				throw new Error(`disconnect failed (${response.status})`)
			}
			await mutate(STATUS_URL)
		} finally {
			setBusy(false)
		}
	}

	return (
		<section className="rounded-lg border bg-card p-6 shadow-sm">
			<div className="flex items-start justify-between gap-4">
				<div>
					<h2 className="text-lg font-semibold">Google</h2>
					<p className="text-sm text-muted-foreground">
						Gmail (read), Calendar, and Drive (read). Used by MCP tools and
						background sync.
					</p>
				</div>
				{isLoading ? (
					<span className="text-sm text-muted-foreground">Loading…</span>
				) : data?.connected ? (
					<Button
						variant="outline"
						onClick={disconnect}
						disabled={busy}
					>
						{busy ? "Disconnecting…" : "Disconnect"}
					</Button>
				) : (
					<Button asChild>
						<a href={START_URL}>Connect Google</a>
					</Button>
				)}
			</div>
			{error && (
				<p className="mt-4 text-sm text-red-500">
					Couldn&apos;t load status: {String(error.message ?? error)}
				</p>
			)}
			{data?.connected && (
				<dl className="mt-4 grid grid-cols-1 gap-2 text-sm sm:grid-cols-2">
					<div>
						<dt className="text-muted-foreground">Account</dt>
						<dd className="font-mono">{data.account_email ?? "unknown"}</dd>
					</div>
					<div>
						<dt className="text-muted-foreground">Connected</dt>
						<dd>{formatDate(data.updated_at)}</dd>
					</div>
					<div className="sm:col-span-2">
						<dt className="text-muted-foreground">Scopes</dt>
						<dd className="mt-1 flex flex-wrap gap-1">
							{data.scopes.map((scope) => (
								<span
									key={scope}
									className="rounded bg-muted px-2 py-0.5 font-mono text-xs"
								>
									{scope.replace("https://www.googleapis.com/auth/", "")}
								</span>
							))}
						</dd>
					</div>
				</dl>
			)}
		</section>
	)
}

"use client"

import useSWR, { mutate } from "swr"
import { useState } from "react"
import { Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"

interface TokenSummary {
	id: string
	name: string
	subject: string | null
	created_at: number
	last_used_at: number | null
	expires_at: number | null
}

interface ListTokensResponse {
	tokens: TokenSummary[]
}

const TOKENS_ENDPOINT = "/api/tokens"

async function fetcher(url: string): Promise<ListTokensResponse> {
	const response = await fetch(url)
	if (!response.ok) {
		throw new Error(`failed to load tokens (${response.status})`)
	}
	return response.json()
}

function formatRelative(ms: number | null | undefined): string {
	if (!ms) return "—"
	const date = new Date(ms)
	return date.toLocaleString()
}

export function TokensList() {
	const { data, error, isLoading } = useSWR<ListTokensResponse>(
		TOKENS_ENDPOINT,
		fetcher
	)
	const [revoking, setRevoking] = useState<string | null>(null)

	async function handleRevoke(id: string) {
		if (!confirm("Revoke this token? MCP clients using it will lose access.")) {
			return
		}
		setRevoking(id)
		try {
			const response = await fetch(`/api/tokens/${encodeURIComponent(id)}`, {
				method: "DELETE",
			})
			if (!response.ok) {
				const body = (await response.json().catch(() => ({}))) as {
					error?: string
				}
				alert(`Revoke failed: ${body.error ?? response.status}`)
				return
			}
			await mutate(TOKENS_ENDPOINT)
		} finally {
			setRevoking(null)
		}
	}

	if (isLoading) {
		return <p className="text-sm text-muted-foreground">Loading…</p>
	}
	if (error) {
		return (
			<p className="text-sm text-destructive">
				Failed to load tokens: {(error as Error).message}
			</p>
		)
	}
	const tokens = data?.tokens ?? []
	if (tokens.length === 0) {
		return (
			<p className="text-sm text-muted-foreground">
				No tokens yet. Run <code className="font-mono">mcp-stdio login</code>{" "}
				from your MCP host to create one.
			</p>
		)
	}

	return (
		<div className="divide-y rounded-md border">
			{tokens.map((token) => (
				<div
					key={token.id}
					className="flex items-center justify-between gap-4 px-4 py-3 text-sm"
				>
					<div className="min-w-0 flex-1 space-y-1">
						<p className="font-medium truncate">{token.name}</p>
						<p className="text-xs text-muted-foreground">
							Created {formatRelative(token.created_at)} · Last used{" "}
							{formatRelative(token.last_used_at)}
							{token.expires_at
								? ` · Expires ${formatRelative(token.expires_at)}`
								: ""}
						</p>
						<p className="font-mono text-xs text-muted-foreground">
							{token.id}
						</p>
					</div>
					<Button
						variant="ghost"
						size="icon"
						disabled={revoking === token.id}
						onClick={() => handleRevoke(token.id)}
						title="Revoke"
					>
						<Trash2 className="h-4 w-4" />
					</Button>
				</div>
			))}
		</div>
	)
}

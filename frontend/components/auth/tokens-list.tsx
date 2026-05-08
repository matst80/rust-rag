"use client"

import useSWR, { mutate } from "swr"
import { useState } from "react"
import { Trash2, Plus, Copy, Check } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

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

interface CreateTokenResponse {
	token: string
	id: string
	name: string
	expires_at: number | null
}

const TOKENS_ENDPOINT = "/api/auth/tokens"

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

function NewTokenBanner({
	token,
	onDismiss,
}: {
	token: CreateTokenResponse
	onDismiss: () => void
}) {
	const [copied, setCopied] = useState(false)

	async function handleCopy() {
		await navigator.clipboard.writeText(token.token)
		setCopied(true)
		setTimeout(() => setCopied(false), 2000)
	}

	return (
		<div className="rounded-md border border-green-500 bg-green-50 dark:bg-green-950 p-4 space-y-2">
			<p className="text-sm font-medium text-green-800 dark:text-green-200">
				Token created — copy it now, it won&apos;t be shown again.
			</p>
			<div className="flex items-center gap-2">
				<code className="flex-1 rounded bg-white dark:bg-black border px-2 py-1 text-xs font-mono break-all">
					{token.token}
				</code>
				<Button variant="outline" size="icon" onClick={handleCopy} title="Copy token">
					{copied ? <Check className="h-4 w-4 text-green-600" /> : <Copy className="h-4 w-4" />}
				</Button>
			</div>
			<p className="text-xs text-muted-foreground">
				Name: <strong>{token.name}</strong>
				{token.expires_at ? ` · Expires ${formatRelative(token.expires_at)}` : ""}
			</p>
			<Button variant="outline" size="sm" onClick={onDismiss}>
				Done
			</Button>
		</div>
	)
}

export function TokensList() {
	const { data, error, isLoading } = useSWR<ListTokensResponse>(
		TOKENS_ENDPOINT,
		fetcher
	)
	const [revoking, setRevoking] = useState<string | null>(null)
	const [creating, setCreating] = useState(false)
	const [newName, setNewName] = useState("")
	const [newToken, setNewToken] = useState<CreateTokenResponse | null>(null)

	async function handleRevoke(id: string) {
		if (!confirm("Revoke this token? MCP clients using it will lose access.")) {
			return
		}
		setRevoking(id)
		try {
			const response = await fetch(`/api/auth/tokens/${encodeURIComponent(id)}`, {
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

	async function handleCreate(e: React.FormEvent) {
		e.preventDefault()
		setCreating(true)
		try {
			const response = await fetch(TOKENS_ENDPOINT, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ name: newName }),
			})
			if (!response.ok) {
				const body = (await response.json().catch(() => ({}))) as {
					error?: string
				}
				alert(`Create failed: ${body.error ?? response.status}`)
				return
			}
			const result = (await response.json()) as CreateTokenResponse
			setNewToken(result)
			setNewName("")
			await mutate(TOKENS_ENDPOINT)
		} finally {
			setCreating(false)
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

	return (
		<div className="space-y-4">
			{newToken && (
				<NewTokenBanner token={newToken} onDismiss={() => setNewToken(null)} />
			)}

			<form onSubmit={handleCreate} className="flex items-center gap-2">
				<Input
					placeholder="Token name (e.g. my-http-mcp-client)"
					value={newName}
					onChange={(e) => setNewName(e.target.value)}
					className="flex-1"
				/>
				<Button type="submit" disabled={creating || !newName.trim()} size="sm">
					<Plus className="h-4 w-4 mr-1" />
					{creating ? "Creating…" : "Create token"}
				</Button>
			</form>

			{tokens.length === 0 ? (
				<p className="text-sm text-muted-foreground">
					No tokens yet. Create one above or run{" "}
					<code className="font-mono">mcp-stdio login</code> from your MCP host.
				</p>
			) : (
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
			)}
		</div>
	)
}

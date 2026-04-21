"use client"

import Link from "next/link"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

interface DeviceApproveFormProps {
	initialUserCode: string
}

interface VerifyResponse {
	user_code: string
	status: string
	client_name?: string | null
	expires_at: number
}

interface ApproveResponse {
	token_id: string
	user_code: string
}

export function DeviceApproveForm({ initialUserCode }: DeviceApproveFormProps) {
	const [userCode, setUserCode] = useState(initialUserCode.toUpperCase())
	const [name, setName] = useState("")
	const [submitting, setSubmitting] = useState(false)
	const [result, setResult] = useState<
		| { kind: "success"; tokenId: string }
		| { kind: "error"; message: string }
		| null
	>(null)
	const [verifying, setVerifying] = useState(false)
	const [verified, setVerified] = useState<VerifyResponse | null>(null)

	async function handleVerify() {
		if (!userCode.trim()) return
		setVerifying(true)
		setVerified(null)
		try {
			const response = await fetch(
				`/api/device/verify?user_code=${encodeURIComponent(userCode.trim())}`
			)
			if (response.ok) {
				setVerified((await response.json()) as VerifyResponse)
			} else {
				const body = (await response.json().catch(() => ({}))) as {
					error?: string
				}
				setResult({ kind: "error", message: body.error ?? "not found" })
			}
		} catch (error) {
			setResult({
				kind: "error",
				message: error instanceof Error ? error.message : "network error",
			})
		} finally {
			setVerifying(false)
		}
	}

	async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
		event.preventDefault()
		const trimmed = userCode.trim()
		if (!trimmed) return

		setSubmitting(true)
		setResult(null)
		try {
			const response = await fetch("/api/device/approve", {
				method: "POST",
				headers: { "content-type": "application/json" },
				body: JSON.stringify({
					user_code: trimmed,
					name: name.trim() || null,
				}),
			})
			if (response.ok) {
				const data = (await response.json()) as ApproveResponse
				setResult({ kind: "success", tokenId: data.token_id })
			} else {
				const body = (await response.json().catch(() => ({}))) as {
					error?: string
				}
				setResult({
					kind: "error",
					message: body.error ?? `request failed with ${response.status}`,
				})
			}
		} catch (error) {
			setResult({
				kind: "error",
				message: error instanceof Error ? error.message : "network error",
			})
		} finally {
			setSubmitting(false)
		}
	}

	if (result?.kind === "success") {
		return (
			<div className="rounded-md border border-primary/30 bg-primary/5 p-4 text-sm">
				<p className="font-medium">Approved.</p>
				<p className="mt-2 text-muted-foreground">
					Token id <span className="font-mono">{result.tokenId}</span> is now
					bound to your account. The MCP client should pick up the bearer
					automatically — you can close this window.
				</p>
				<div className="mt-4">
					<Link
						href="/auth/tokens"
						className="text-primary underline-offset-4 hover:underline"
					>
						Manage tokens →
					</Link>
				</div>
			</div>
		)
	}

	return (
		<form className="space-y-4" onSubmit={handleSubmit}>
			<div className="space-y-2">
				<Label htmlFor="user_code">User code</Label>
				<Input
					id="user_code"
					autoFocus
					required
					value={userCode}
					onChange={(event) =>
						setUserCode(event.target.value.toUpperCase())
					}
					onBlur={handleVerify}
					placeholder="ABCD-EFGH"
					className="font-mono tracking-widest uppercase"
				/>
				{verifying && (
					<p className="text-xs text-muted-foreground">Looking up…</p>
				)}
				{verified && (
					<p className="text-xs text-muted-foreground">
						Status: <span className="font-medium">{verified.status}</span>
						{verified.client_name ? ` — ${verified.client_name}` : ""}
					</p>
				)}
			</div>
			<div className="space-y-2">
				<Label htmlFor="name">Token label (optional)</Label>
				<Input
					id="name"
					value={name}
					onChange={(event) => setName(event.target.value)}
					placeholder="claude-code on laptop"
				/>
			</div>
			{result?.kind === "error" && (
				<p className="text-sm text-destructive">Error: {result.message}</p>
			)}
			<Button type="submit" disabled={submitting || !userCode.trim()}>
				{submitting ? "Approving…" : "Approve"}
			</Button>
		</form>
	)
}

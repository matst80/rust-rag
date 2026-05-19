"use client"

import useSWR, { mutate } from "swr"
import { useState } from "react"
import { Button } from "@/components/ui/button"

const STATUS_URL = "/api/push/subscriptions"
const VAPID_URL = "/api/push/vapid-public-key"
const SUBSCRIBE_URL = "/api/push/subscribe"

interface SubscriptionView {
	id: string
	endpoint: string
	user_agent: string | null
	created_at: number
	last_used_at: number | null
}

interface ListResponse {
	subscriptions: SubscriptionView[]
}

async function fetcher(url: string): Promise<ListResponse> {
	const response = await fetch(url, { credentials: "same-origin" })
	if (!response.ok) {
		throw new Error(`failed to load subscriptions (${response.status})`)
	}
	return response.json()
}

function urlBase64ToUint8Array(base64String: string): Uint8Array {
	const padding = "=".repeat((4 - (base64String.length % 4)) % 4)
	const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/")
	const rawData = atob(base64)
	const out = new Uint8Array(rawData.length)
	for (let i = 0; i < rawData.length; i++) out[i] = rawData.charCodeAt(i)
	return out
}

function formatDate(ms: number | null | undefined): string {
	if (!ms) return "—"
	return new Date(ms).toLocaleString()
}

function shortEndpoint(endpoint: string): string {
	try {
		const url = new URL(endpoint)
		return `${url.hostname}…${endpoint.slice(-8)}`
	} catch {
		return endpoint.slice(0, 24) + "…"
	}
}

export function NotificationsCard() {
	const { data, error, isLoading } = useSWR(STATUS_URL, fetcher)
	const [busy, setBusy] = useState(false)
	const [errMsg, setErrMsg] = useState<string | null>(null)

	const supported =
		typeof window !== "undefined" &&
		"serviceWorker" in navigator &&
		"PushManager" in window

	async function enable() {
		setBusy(true)
		setErrMsg(null)
		try {
			if (!supported) throw new Error("push not supported by this browser")

			const permission = await Notification.requestPermission()
			if (permission !== "granted") {
				throw new Error(`notification permission ${permission}`)
			}

			const reg = await navigator.serviceWorker.register("/sw.js")
			await navigator.serviceWorker.ready

			const vapidResp = await fetch(VAPID_URL, { credentials: "same-origin" })
			if (!vapidResp.ok) {
				throw new Error(
					`server has no VAPID key configured (${vapidResp.status})`,
				)
			}
			const { public_key } = (await vapidResp.json()) as { public_key: string }

			const sub = await reg.pushManager.subscribe({
				userVisibleOnly: true,
				applicationServerKey: urlBase64ToUint8Array(public_key),
			})

			const json = sub.toJSON() as {
				endpoint: string
				keys: { p256dh: string; auth: string }
			}
			const postResp = await fetch(SUBSCRIBE_URL, {
				method: "POST",
				credentials: "same-origin",
				headers: { "content-type": "application/json" },
				body: JSON.stringify({
					endpoint: json.endpoint,
					keys: { p256dh: json.keys.p256dh, auth: json.keys.auth },
				}),
			})
			if (!postResp.ok) {
				throw new Error(`subscribe failed (${postResp.status})`)
			}
			await mutate(STATUS_URL)
		} catch (e: unknown) {
			setErrMsg(e instanceof Error ? e.message : String(e))
		} finally {
			setBusy(false)
		}
	}

	async function disable(id: string) {
		setBusy(true)
		setErrMsg(null)
		try {
			const resp = await fetch(`${STATUS_URL}/${encodeURIComponent(id)}`, {
				method: "DELETE",
				credentials: "same-origin",
			})
			if (!resp.ok) throw new Error(`delete failed (${resp.status})`)

			// Best-effort: also unsubscribe from the browser side.
			if (supported) {
				const reg = await navigator.serviceWorker.getRegistration()
				const local = await reg?.pushManager.getSubscription()
				await local?.unsubscribe()
			}
			await mutate(STATUS_URL)
		} catch (e: unknown) {
			setErrMsg(e instanceof Error ? e.message : String(e))
		} finally {
			setBusy(false)
		}
	}

	const hasAny = (data?.subscriptions ?? []).length > 0

	return (
		<section className="rounded-lg border bg-card p-6 shadow-sm">
			<div className="flex items-start justify-between gap-4">
				<div>
					<h2 className="text-lg font-semibold">Web Push notifications</h2>
					<p className="text-sm text-muted-foreground">
						Allow agents to notify you on this device for reminders, important
						mail, and long-running task completions.
					</p>
				</div>
				{isLoading ? (
					<span className="text-sm text-muted-foreground">Loading…</span>
				) : (
					<Button onClick={enable} disabled={busy || !supported}>
						{busy ? "Working…" : "Enable on this device"}
					</Button>
				)}
			</div>
			{!supported && (
				<p className="mt-4 text-sm text-amber-500">
					This browser does not support Web Push.
				</p>
			)}
			{error && (
				<p className="mt-4 text-sm text-red-500">
					Couldn&apos;t load subscriptions: {String(error.message ?? error)}
				</p>
			)}
			{errMsg && <p className="mt-4 text-sm text-red-500">{errMsg}</p>}
			{hasAny && (
				<ul className="mt-4 divide-y border-t">
					{data!.subscriptions.map((s) => (
						<li
							key={s.id}
							className="flex items-center justify-between gap-4 py-3"
						>
							<div className="text-sm">
								<div className="font-mono">{shortEndpoint(s.endpoint)}</div>
								<div className="text-muted-foreground">
									{s.user_agent ?? "unknown device"} · subscribed{" "}
									{formatDate(s.created_at)}
								</div>
							</div>
							<Button
								variant="outline"
								size="sm"
								onClick={() => disable(s.id)}
								disabled={busy}
							>
								Remove
							</Button>
						</li>
					))}
				</ul>
			)}
		</section>
	)
}

import { NextRequest, NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { readSessionFromRequest } from "@/lib/auth/session"

export const dynamic = "force-dynamic"

interface AcpInstance {
	name: string
	url: string
}

interface InstancesResponse {
	instances: AcpInstance[]
	active: string | null
}

export async function GET(request: NextRequest) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ error: "unauthorized" }, { status: 401 })
	}

	const token = process.env.ACP_WS_TOKEN ?? process.env.TELEGRAM_ACP_WS_TOKEN ?? null
	if (!token) {
		return NextResponse.json(
			{ error: "ACP WS token not configured (set ACP_WS_TOKEN)" },
			{ status: 503 },
		)
	}

	// Prefer discovered active instance from backend; fall back to env URL.
	const fallback = process.env.ACP_WS_PUBLIC_URL ?? process.env.ACP_WS_URL ?? null

	let url: string | null = null
	try {
		const cfg = getAuthConfig()
		const headers: Record<string, string> = {}
		if (cfg.backendApiKey) headers["x-api-key"] = cfg.backendApiKey
		const res = await fetch(`${cfg.backendApiUrl}/api/acp/instances`, {
			headers,
			cache: "no-store",
		})
		if (res.ok) {
			const data = (await res.json()) as InstancesResponse
			const active = data.instances.find((i) => i.name === data.active)
			url = active?.url ?? data.instances[0]?.url ?? null
		}
	} catch (err) {
		console.warn("acp/config: instance lookup failed", err)
	}

	url = url ?? fallback
	if (!url) {
		return NextResponse.json(
			{ error: "ACP WS not discovered and no ACP_WS_PUBLIC_URL fallback set" },
			{ status: 503 },
		)
	}

	return NextResponse.json({ url, token })
}

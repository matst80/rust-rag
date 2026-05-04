import { NextRequest, NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { readSessionFromRequest } from "@/lib/auth/session"

export const dynamic = "force-dynamic"

export async function GET(request: NextRequest) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ error: "unauthorized" }, { status: 401 })
	}
	const cfg = getAuthConfig()
	const headers: Record<string, string> = {}
	if (cfg.backendApiKey) headers["x-api-key"] = cfg.backendApiKey
	try {
		const res = await fetch(`${cfg.backendApiUrl}/api/acp/instances`, {
			headers,
			cache: "no-store",
		})
		const text = await res.text()
		return new NextResponse(text, {
			status: res.status,
			headers: { "content-type": res.headers.get("content-type") ?? "application/json" },
		})
	} catch (err) {
		return NextResponse.json({ error: String(err) }, { status: 502 })
	}
}

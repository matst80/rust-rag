import { NextRequest, NextResponse } from "next/server"
import { readSessionFromRequest } from "@/lib/auth/session"

export const dynamic = "force-dynamic"

// Browser ACP WebSockets now go through the Rust backend's `/api/acp/ws`
// proxy, terminated on the same TLS origin as this BFF. The browser
// connects with its session cookie — no daemon URL or bearer is leaked
// into the client. Daemon discovery + selection still happens server-side
// via /api/acp/instances and /api/acp/select.
export async function GET(request: NextRequest) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ error: "unauthorized" }, { status: 401 })
	}
	return NextResponse.json({ url: "/api/acp/ws", token: null })
}

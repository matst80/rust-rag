import { NextRequest, NextResponse } from "next/server"
import { readSessionFromRequest } from "@/lib/auth/session"

export const dynamic = "force-dynamic"

export async function GET(request: NextRequest) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ error: "unauthorized" }, { status: 401 })
	}

	const url = process.env.ACP_WS_PUBLIC_URL ?? process.env.ACP_WS_URL ?? null
	const token = process.env.ACP_WS_TOKEN ?? process.env.TELEGRAM_ACP_WS_TOKEN ?? null
	if (!url || !token) {
		return NextResponse.json(
			{ error: "ACP WS not configured (set ACP_WS_PUBLIC_URL + ACP_WS_TOKEN)" },
			{ status: 503 },
		)
	}

	return NextResponse.json({ url, token })
}

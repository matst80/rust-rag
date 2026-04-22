import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { readSessionFromRequest, SESSION_COOKIE_NAME } from "@/lib/auth/session"

export const runtime = "nodejs"

export async function GET(request: NextRequest) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ error: "unauthenticated" }, { status: 401 })
	}

	const config = getAuthConfig()
	const sessionCookie = request.cookies.get(SESSION_COOKIE_NAME)?.value
	if (!sessionCookie) {
		return NextResponse.json({ error: "unauthenticated" }, { status: 401 })
	}

	const userCode = request.nextUrl.searchParams.get("user_code") ?? ""
	if (!userCode) {
		return NextResponse.json({ error: "user_code required" }, { status: 400 })
	}

	try {
		const upstream = await fetch(
			`${config.backendApiUrl}/auth/device/verify?user_code=${encodeURIComponent(userCode)}`,
			{
				headers: {
					cookie: `${SESSION_COOKIE_NAME}=${sessionCookie}`,
				},
			}
		)

		if (!upstream.ok) {
			const errorText = await upstream.text().catch(() => "no body")
			console.error(`Upstream error (${upstream.status}) from backend: ${errorText}`)
		}

		const text = await upstream.text()
		return new NextResponse(text, {
			status: upstream.status,
			headers: {
				"content-type": upstream.headers.get("content-type") ?? "application/json",
			},
		})
	} catch (error) {
		console.error("Backend fetch failed in /api/device/verify:", error)
		return NextResponse.json({ error: "Backend communication failed" }, { status: 500 })
	}
}

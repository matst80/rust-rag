import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { readSessionFromRequest, SESSION_COOKIE_NAME } from "@/lib/auth/session"

export const runtime = "nodejs"

export async function DELETE(
	request: NextRequest,
	{ params }: { params: Promise<{ id: string }> }
) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ error: "unauthenticated" }, { status: 401 })
	}

	const config = getAuthConfig()
	const sessionCookie = request.cookies.get(SESSION_COOKIE_NAME)?.value
	if (!sessionCookie) {
		return NextResponse.json({ error: "unauthenticated" }, { status: 401 })
	}

	const { id } = await params
	const upstream = await fetch(
		`${config.backendApiUrl}/auth/tokens/${encodeURIComponent(id)}`,
		{
			method: "DELETE",
			headers: {
				cookie: `${SESSION_COOKIE_NAME}=${sessionCookie}`,
			},
		}
	)

	const text = await upstream.text()
	return new NextResponse(text, {
		status: upstream.status,
		headers: {
			"content-type": upstream.headers.get("content-type") ?? "application/json",
		},
	})
}

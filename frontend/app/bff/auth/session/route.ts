import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { readSessionFromRequest } from "@/lib/auth/session"

export const runtime = "nodejs"

export async function GET(request: NextRequest) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ authenticated: false }, { status: 200 })
	}

	return NextResponse.json({
		authenticated: true,
		user: {
			sub: session.sub,
			name: session.name,
			email: session.email,
			preferred_username: session.preferred_username,
		},
	})
}

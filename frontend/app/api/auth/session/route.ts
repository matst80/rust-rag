import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { readSessionFromRequest } from "@/lib/auth/session"

import { getAuthConfig } from "@/lib/auth/config"

export const runtime = "nodejs"

export async function GET(request: NextRequest) {
	const config = getAuthConfig()
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({
			authenticated: false,
			authEnabled: config.authEnabled,
		})
	}

	return NextResponse.json({
		authenticated: true,
		authEnabled: config.authEnabled,
		user: {
			name: session.name,
			email: session.email,
			preferred_username: session.preferred_username,
		},
	})
}
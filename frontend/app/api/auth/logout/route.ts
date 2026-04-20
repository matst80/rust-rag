import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { clearSessionCookie } from "@/lib/auth/session"

export const runtime = "nodejs"

export async function GET(_request: NextRequest) {
	const config = getAuthConfig()
	const response = NextResponse.redirect(new URL("/", config.appBaseUrl))
	clearSessionCookie(response)
	return response
}
import { NextResponse } from "next/server"
import { clearRefreshCookie, clearSessionCookie } from "@/lib/auth/session"

export const runtime = "nodejs"

export async function GET() {
	const response = NextResponse.redirect(new URL("/", process.env.APP_BASE_URL || "http://localhost:3000"))
	clearSessionCookie(response)
	clearRefreshCookie(response)
	return response
}

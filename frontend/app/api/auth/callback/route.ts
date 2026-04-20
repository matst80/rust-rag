import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { setSessionCookie } from "@/lib/auth/session"
import { exchangeCodeForTokens, fetchUserInfo } from "@/lib/auth/zitadel"

export const runtime = "nodejs"

const AUTH_STATE_COOKIE = "rag_auth_state"
const AUTH_PKCE_COOKIE = "rag_auth_pkce"
const AUTH_RETURN_TO_COOKIE = "rag_auth_return_to"

function clearTemporaryCookies(response: NextResponse) {
	for (const name of [AUTH_STATE_COOKIE, AUTH_PKCE_COOKIE, AUTH_RETURN_TO_COOKIE]) {
		response.cookies.set({
			name,
			value: "",
			httpOnly: true,
			sameSite: "lax",
			secure: process.env.NODE_ENV === "production",
			path: "/",
			maxAge: 0,
		})
	}
}

export async function GET(request: NextRequest) {
	const code = request.nextUrl.searchParams.get("code")
	const state = request.nextUrl.searchParams.get("state")
	const storedState = request.cookies.get(AUTH_STATE_COOKIE)?.value
	const codeVerifier = request.cookies.get(AUTH_PKCE_COOKIE)?.value
	const returnTo = request.cookies.get(AUTH_RETURN_TO_COOKIE)?.value ?? "/"

	if (!code || !state || !storedState || !codeVerifier || state !== storedState) {
		return NextResponse.json({ error: "invalid authentication callback" }, { status: 400 })
	}

	const config = getAuthConfig()
	const tokens = await exchangeCodeForTokens(code, codeVerifier)
	const user = await fetchUserInfo(tokens)
	const response = NextResponse.redirect(new URL(returnTo, config.appBaseUrl))

	await setSessionCookie(
		response,
		{
			sub: user.sub,
			name: user.name,
			email: user.email,
			preferred_username: user.preferred_username,
		},
		config.sessionMaxAgeSeconds
	)
	clearTemporaryCookies(response)

	return response
}
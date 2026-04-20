import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { createPkceChallenge, createRandomToken, getDiscoveryDocument } from "@/lib/auth/zitadel"

export const runtime = "nodejs"

const AUTH_STATE_COOKIE = "rag_auth_state"
const AUTH_PKCE_COOKIE = "rag_auth_pkce"
const AUTH_RETURN_TO_COOKIE = "rag_auth_return_to"

function isSecureCookie() {
	return process.env.NODE_ENV === "production"
}

function normalizeReturnTo(value: string | null) {
	if (!value || !value.startsWith("/") || value.startsWith("//")) {
		return "/"
	}

	return value
}

export async function GET(request: NextRequest) {
	try {
		const config = getAuthConfig()
		const discovery = await getDiscoveryDocument()
		const state = createRandomToken()
		const codeVerifier = createRandomToken(48)
		const codeChallenge = await createPkceChallenge(codeVerifier)
		const returnTo = normalizeReturnTo(request.nextUrl.searchParams.get("returnTo"))

		const url = new URL(discovery.authorization_endpoint)
		url.searchParams.set("client_id", config.clientId)
		url.searchParams.set("response_type", "code")
		url.searchParams.set("redirect_uri", config.redirectUri)
		url.searchParams.set("scope", config.scopes)
		url.searchParams.set("state", state)
		url.searchParams.set("code_challenge", codeChallenge)
		url.searchParams.set("code_challenge_method", "S256")

		const response = NextResponse.redirect(url)
		for (const [name, value] of [
			[AUTH_STATE_COOKIE, state],
			[AUTH_PKCE_COOKIE, codeVerifier],
			[AUTH_RETURN_TO_COOKIE, returnTo],
		] as const) {
			response.cookies.set({
				name,
				value,
				httpOnly: true,
				sameSite: "lax",
				secure: isSecureCookie(),
				path: "/",
				maxAge: 10 * 60,
			})
		}

		return response
	} catch (error) {
		const message = error instanceof Error ? error.message : "unknown auth setup error"
		console.error("auth login setup failed", error)
		return NextResponse.json(
			{
				error: process.env.NODE_ENV === "production" ? "authentication setup failed" : message,
			},
			{ status: 500 }
		)
	}
}
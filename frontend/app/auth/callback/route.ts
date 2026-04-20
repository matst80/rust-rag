import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { setSessionCookie } from "@/lib/auth/session"
import { exchangeCodeForToken, getDiscoveryDocument, verifyIdToken } from "@/lib/auth/zitadel"

export const runtime = "nodejs"

export async function GET(request: NextRequest) {
	const config = getAuthConfig()
	const searchParams = request.nextUrl.searchParams
	const code = searchParams.get("code")
	const state = searchParams.get("state")
	const error = searchParams.get("error")

	if (error) {
		console.error("auth callback error from provider", error)
		return NextResponse.json({ error }, { status: 400 })
	}

	if (!code || !state) {
		return NextResponse.json({ error: "missing code or state" }, { status: 400 })
	}

	const storedState = request.cookies.get("rag_auth_state")?.value
	const nonce = request.cookies.get("rag_auth_nonce")?.value
	const codeVerifier = request.cookies.get("rag_auth_code_verifier")?.value
	const returnTo = request.cookies.get("rag_auth_return_to")?.value || "/"

	if (!storedState || state !== storedState || !nonce || !codeVerifier) {
		return NextResponse.json({ error: "invalid or expired state" }, { status: 400 })
	}

	try {
		const discovery = await getDiscoveryDocument(config.issuer)

		const tokenResponse = await exchangeCodeForToken(
			code,
			codeVerifier,
			config.clientId,
			config.clientSecret,
			config.redirectUri,
			discovery.token_endpoint
		)

		const user = await verifyIdToken(
			tokenResponse.id_token,
			discovery.jwks_uri,
			discovery.issuer,
			config.clientId
		)

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
	} catch (error) {
		console.error("auth callback failed", error)
		const message = error instanceof Error ? error.message : "Internal Server Error"
		return NextResponse.json(
			{
				error: process.env.NODE_ENV === "production" ? "authentication failed" : message,
			},
			{ status: 500 }
		)
	}
}

function clearTemporaryCookies(response: NextResponse) {
	const options = { path: "/", maxAge: 0 }
	response.cookies.set("rag_auth_state", "", options)
	response.cookies.set("rag_auth_nonce", "", options)
	response.cookies.set("rag_auth_code_verifier", "", options)
	response.cookies.set("rag_auth_return_to", "", options)
}

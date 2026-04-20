import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { createPkceChallenge, createRandomToken, getDiscoveryDocument } from "@/lib/auth/zitadel"

export const runtime = "nodejs"

export async function GET(request: NextRequest) {
	const config = getAuthConfig()

	try {
		const discovery = await getDiscoveryDocument(config.issuer)

		const state = createRandomToken()
		const nonce = createRandomToken()
		const codeVerifier = createRandomToken(64)
		const codeChallenge = await createPkceChallenge(codeVerifier)

		const searchParams = request.nextUrl.searchParams
		const returnTo = searchParams.get("returnTo") || "/"

		const authUrl = new URL(discovery.authorization_endpoint)
		authUrl.searchParams.set("response_type", "code")
		authUrl.searchParams.set("client_id", config.clientId)
		authUrl.searchParams.set("state", state)
		authUrl.searchParams.set("code_challenge", codeChallenge)
		authUrl.searchParams.set("code_challenge_method", "S256")
		authUrl.searchParams.set("redirect_uri", config.redirectUri)
		authUrl.searchParams.set("scope", config.scopes)
		authUrl.searchParams.set("nonce", nonce)

		const response = NextResponse.redirect(authUrl)

		// Store OIDC flow state in temporary cookies
		const cookieOptions = {
			httpOnly: true,
			secure: config.appBaseUrl.startsWith("https"),
			sameSite: "lax" as const,
			path: "/",
			maxAge: 600, // 10 minutes
		}

		response.cookies.set("rag_auth_state", state, cookieOptions)
		response.cookies.set("rag_auth_nonce", nonce, cookieOptions)
		response.cookies.set("rag_auth_code_verifier", codeVerifier, cookieOptions)
		response.cookies.set("rag_auth_return_to", returnTo, cookieOptions)

		return response
	} catch (error) {
		const message = error instanceof Error ? error.message : "Internal Server Error"
		console.error("auth login setup failed", error)
		return NextResponse.json(
			{
				error: process.env.NODE_ENV === "production" ? "authentication setup failed" : message,
			},
			{ status: 500 }
		)
	}
}

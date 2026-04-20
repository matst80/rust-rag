import { jwtVerify, createRemoteJWKSet } from "jose"

export interface DiscoveryDocument {
	issuer: string
	authorization_endpoint: string
	token_endpoint: string
	userinfo_endpoint: string
	jwks_uri: string
}

export interface ZitadelTokenResponse {
	access_token: string
	id_token: string
	token_type: string
	expires_in: number
	scope: string
}

export interface IdTokenClaims {
	sub: string
	name?: string
	email?: string
	preferred_username?: string
	exp: number
	aud: string | string[]
	iss: string
}

export async function getDiscoveryDocument(issuer: string): Promise<DiscoveryDocument> {
	const response = await fetch(`${issuer.replace(/\/$/, "")}/.well-known/openid-configuration`)
	if (!response.ok) {
		throw new Error(`failed to fetch discovery document: ${response.statusText}`)
	}
	return response.json()
}

export async function exchangeCodeForToken(
	code: string,
	codeVerifier: string,
	clientId: string,
	clientSecret: string,
	redirectUri: string,
	tokenEndpoint: string
): Promise<ZitadelTokenResponse> {
	const params = new URLSearchParams({
		grant_type: "authorization_code",
		code,
		redirect_uri: redirectUri,
		client_id: clientId,
		client_secret: clientSecret,
		code_verifier: codeVerifier,
	})

	const response = await fetch(tokenEndpoint, {
		method: "POST",
		headers: {
			"Content-Type": "application/x-www-form-urlencoded",
		},
		body: params.toString(),
	})

	if (!response.ok) {
		const error = await response.text()
		throw new Error(`token exchange failed: ${error}`)
	}

	return response.json()
}

export async function verifyIdToken(
	idToken: string,
	jwksUri: string,
	issuer: string,
	clientId: string
): Promise<IdTokenClaims> {
	const JWKS = createRemoteJWKSet(new URL(jwksUri))
	const { payload } = await jwtVerify(idToken, JWKS, {
		issuer,
		audience: clientId,
	})

	return payload as unknown as IdTokenClaims
}

export async function createPkceChallenge(verifier: string): Promise<string> {
	const encoder = new TextEncoder()
	const data = encoder.encode(verifier)
	const hash = await crypto.subtle.digest("SHA-256", data)
	return btoa(String.fromCharCode(...new Uint8Array(hash)))
		.replace(/\+/g, "-")
		.replace(/\//g, "_")
		.replace(/=+$/, "")
}

export function createRandomToken(length = 32): string {
	const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~"
	let result = ""
	const bytes = new Uint8Array(length)
	crypto.getRandomValues(bytes)
	for (let i = 0; i < length; i++) {
		result += chars.charAt(bytes[i] % chars.length)
	}
	return result
}

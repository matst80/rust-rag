import { decodeJwt } from "jose"
import { getAuthConfig } from "@/lib/auth/config"

interface DiscoveryDocument {
	authorization_endpoint: string
	token_endpoint: string
	userinfo_endpoint?: string
	end_session_endpoint?: string
}

interface TokenResponse {
	access_token: string
	id_token?: string
	token_type: string
	expires_in?: number
}

export interface ZitadelUserInfo {
	sub: string
	name?: string
	email?: string
	preferred_username?: string
}

let discoveryPromise: Promise<DiscoveryDocument> | undefined

function ensureTrailingSlash(value: string) {
	return value.endsWith("/") ? value : `${value}/`
}

function bytesToBase64Url(bytes: Uint8Array) {
	let binary = ""
	for (const byte of bytes) {
		binary += String.fromCharCode(byte)
	}

	return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "")
}

export function createRandomToken(byteLength: number = 32) {
	const bytes = new Uint8Array(byteLength)
	crypto.getRandomValues(bytes)
	return bytesToBase64Url(bytes)
}

export async function createPkceChallenge(verifier: string) {
	const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))
	return bytesToBase64Url(new Uint8Array(digest))
}

export async function getDiscoveryDocument() {
	if (!discoveryPromise) {
		const issuer = ensureTrailingSlash(getAuthConfig().issuer)
		discoveryPromise = fetch(new URL(".well-known/openid-configuration", issuer), {
			cache: "no-store",
		}).then(async (response) => {
			if (!response.ok) {
				throw new Error(`Failed to load Zitadel discovery document: ${response.status}`)
			}

			return (await response.json()) as DiscoveryDocument
		})
	}

	return discoveryPromise
}

export async function exchangeCodeForTokens(code: string, codeVerifier: string) {
	const config = getAuthConfig()
	const discovery = await getDiscoveryDocument()
	const body = new URLSearchParams({
		grant_type: "authorization_code",
		code,
		redirect_uri: config.redirectUri,
		code_verifier: codeVerifier,
		client_id: config.clientId,
		client_secret: config.clientSecret,
	})

	const response = await fetch(discovery.token_endpoint, {
		method: "POST",
		headers: {
			"Content-Type": "application/x-www-form-urlencoded",
			Accept: "application/json",
		},
		body,
		cache: "no-store",
	})

	if (!response.ok) {
		throw new Error(`Zitadel code exchange failed: ${response.status}`)
	}

	return (await response.json()) as TokenResponse
}

export async function fetchUserInfo(tokens: TokenResponse): Promise<ZitadelUserInfo> {
	const discovery = await getDiscoveryDocument()
	if (discovery.userinfo_endpoint) {
		const response = await fetch(discovery.userinfo_endpoint, {
			headers: {
				Authorization: `Bearer ${tokens.access_token}`,
				Accept: "application/json",
			},
			cache: "no-store",
		})

		if (response.ok) {
			return (await response.json()) as ZitadelUserInfo
		}
	}

	if (!tokens.id_token) {
		throw new Error("Zitadel did not return userinfo or id_token claims")
	}

	const claims = decodeJwt(tokens.id_token)
	if (typeof claims.sub !== "string") {
		throw new Error("Zitadel id_token is missing subject claim")
	}

	return {
		sub: claims.sub,
		name: typeof claims.name === "string" ? claims.name : undefined,
		email: typeof claims.email === "string" ? claims.email : undefined,
		preferred_username:
			typeof claims.preferred_username === "string" ? claims.preferred_username : undefined,
	}
}
import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import {
	REFRESH_COOKIE_NAME,
	SESSION_COOKIE_NAME,
	createRefreshToken,
	createSessionToken,
	readRefreshCookie,
	readSessionFromRequest,
} from "@/lib/auth/session"
import { getDiscoveryDocument, refreshAccessToken } from "@/lib/auth/zitadel"
import { acceptsMarkdown } from "@/lib/start-page"

let discoveryCache: { fetchedAt: number; tokenEndpoint: string } | null = null
const DISCOVERY_TTL_MS = 10 * 60 * 1000

async function getTokenEndpoint(issuer: string): Promise<string> {
	if (discoveryCache && Date.now() - discoveryCache.fetchedAt < DISCOVERY_TTL_MS) {
		return discoveryCache.tokenEndpoint
	}
	const discovery = await getDiscoveryDocument(issuer)
	discoveryCache = { fetchedAt: Date.now(), tokenEndpoint: discovery.token_endpoint }
	return discovery.token_endpoint
}

function isProtectedAppRoute(pathname: string) {
	return pathname === "/" || pathname.startsWith("/entries") || pathname.startsWith("/visualize")
}

export async function proxy(request: NextRequest) {
	const pathname = request.nextUrl.pathname
	const isMarkdown = acceptsMarkdown(request.headers.get("accept"))

	let config
	try {
		config = getAuthConfig()
	} catch {
		// Proceed without auth if config can't be loaded
	}

	let requestHeaders = new Headers(request.headers)
	let newSession: string | null = null
	let newRefresh: string | null = null
	let clearRefreshCookie = false

	if (config?.authEnabled) {
		const hasSession = !!request.cookies.get(SESSION_COOKIE_NAME)?.value
		const refreshRaw = request.cookies.get(REFRESH_COOKIE_NAME)?.value

		if (!hasSession && refreshRaw) {
			const refreshPayload = await readRefreshCookie(refreshRaw)
			if (!refreshPayload) {
				clearRefreshCookie = true
			} else {
				try {
					const tokenEndpoint = await getTokenEndpoint(config.issuer)
					const tokens = await refreshAccessToken(
						refreshPayload.refresh_token,
						config.clientId,
						config.clientSecret,
						tokenEndpoint
					)

					newSession = await createSessionToken(refreshPayload.user, config.sessionMaxAgeSeconds)
					const nextRefreshToken = tokens.refresh_token ?? refreshPayload.refresh_token
					newRefresh = await createRefreshToken(
						{ refresh_token: nextRefreshToken, user: refreshPayload.user },
						config.refreshMaxAgeSeconds
					)

					// Update request headers for downstream handlers in this request
					const cookieParts: string[] = []
					for (const c of request.cookies.getAll()) {
						if (c.name === SESSION_COOKIE_NAME || c.name === REFRESH_COOKIE_NAME) continue
						cookieParts.push(`${c.name}=${c.value}`)
					}
					cookieParts.push(`${SESSION_COOKIE_NAME}=${newSession}`)
					cookieParts.push(`${REFRESH_COOKIE_NAME}=${newRefresh}`)
					requestHeaders.set("cookie", cookieParts.join("; "))

					// Also update request.cookies so that readSessionFromRequest works correctly
					request.cookies.set(SESSION_COOKIE_NAME, newSession)
				} catch (error) {
					console.error("proxy refresh failed", error)
					clearRefreshCookie = true
				}
			}
		}
	}

	// Determine response type: rewrite, redirect, or next
	let response: NextResponse

	if (isMarkdown) {
		if (pathname === "/") {
			const url = request.nextUrl.clone()
			url.pathname = "/startpage.md"
			response = NextResponse.rewrite(url, { request: { headers: requestHeaders } })
		} else if (pathname === "/start-guide") {
			const url = request.nextUrl.clone()
			url.pathname = "/start-guide.md"
			response = NextResponse.rewrite(url, { request: { headers: requestHeaders } })
		} else if (pathname === "/mcp-setup") {
			const url = request.nextUrl.clone()
			url.pathname = "/mcp-setup.md"
			response = NextResponse.rewrite(url, { request: { headers: requestHeaders } })
		} else {
			response = NextResponse.next({ request: { headers: requestHeaders } })
		}
	} else if (isProtectedAppRoute(pathname) && config?.authEnabled) {
		const session = await readSessionFromRequest(request)
		if (!session) {
			const url = request.nextUrl.clone()
			url.pathname = "/auth/login"
			url.searchParams.set("returnTo", `${pathname}${request.nextUrl.search}`)
			response = NextResponse.redirect(url)
		} else {
			response = NextResponse.next({ request: { headers: requestHeaders } })
		}
	} else {
		response = NextResponse.next({ request: { headers: requestHeaders } })
	}

	// Apply cookies to response
	if (config?.authEnabled) {
		const secure = config.appBaseUrl.startsWith("https")
		if (newSession && newRefresh) {
			response.cookies.set({
				name: SESSION_COOKIE_NAME,
				value: newSession,
				httpOnly: true,
				sameSite: "lax",
				secure,
				path: "/",
				maxAge: config.sessionMaxAgeSeconds,
			})
			response.cookies.set({
				name: REFRESH_COOKIE_NAME,
				value: newRefresh,
				httpOnly: true,
				sameSite: "lax",
				secure,
				path: "/",
				maxAge: config.refreshMaxAgeSeconds,
			})
		} else if (clearRefreshCookie) {
			response.cookies.set({
				name: REFRESH_COOKIE_NAME,
				value: "",
				path: "/",
				maxAge: 0,
			})
		}
	}

	return response
}

export const config = {
	matcher: [
		"/",
		"/acp",
		"/acp/:path*",
		"/kanban",
		"/kanban/:path*",
		"/messages",
		"/messages/:path*",
		"/entries",
		"/entries/:path*",
		"/visualize",
		"/visualize/:path*",
		"/start-guide",
		"/mcp-setup",
		"/chat",
		"/wiki",
		"/wiki/:path*",
		"/settings",
		"/settings/:path*",
	],
}

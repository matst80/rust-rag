import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"
import { readSessionFromRequest } from "@/lib/auth/session"

const RESPONSE_HEADERS_TO_SKIP = new Set([
	"connection",
	"content-length",
	"keep-alive",
	"proxy-authenticate",
	"proxy-authorization",
	"te",
	"trailer",
	"transfer-encoding",
	"upgrade",
])

function sanitizeBackendBaseUrl(url: string) {
	return url.endsWith("/") ? url : `${url}/`
}

function copyBackendResponseHeaders(source: Headers) {
	const headers = new Headers()
	for (const [key, value] of source.entries()) {
		if (!RESPONSE_HEADERS_TO_SKIP.has(key.toLowerCase())) {
			headers.set(key, value)
		}
	}
	return headers
}

export async function proxyRagRequest(request: NextRequest, backendPath: string) {
	const session = await readSessionFromRequest(request)
	if (!session) {
		return NextResponse.json({ error: "authentication required" }, { status: 401 })
	}

	const config = getAuthConfig()

	const target = new URL(backendPath.replace(/^\//, ""), sanitizeBackendBaseUrl(config.backendApiUrl))
	target.search = request.nextUrl.search

	const headers = new Headers()
	const accept = request.headers.get("accept")
	const contentType = request.headers.get("content-type")
	if (accept) {
		headers.set("accept", accept)
	}
	if (contentType) {
		headers.set("content-type", contentType)
	}
	if (config.backendApiKey) {
		headers.set("x-api-key", config.backendApiKey)
	}
	headers.set("x-authenticated-user", session.sub)
	if (session.email) {
		headers.set("x-authenticated-email", session.email)
	}

	const response = await fetch(target, {
		method: request.method,
		headers,
		body: request.method === "GET" || request.method === "HEAD" ? undefined : await request.text(),
		cache: "no-store",
		redirect: "manual",
	})

	return new NextResponse(response.body, {
		status: response.status,
		headers: copyBackendResponseHeaders(response.headers),
	})
}
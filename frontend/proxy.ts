import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { readSessionFromRequest } from "@/lib/auth/session"
import { acceptsMarkdown } from "@/lib/start-page"

function isProtectedAppRoute(pathname: string) {
	return pathname === "/" || pathname.startsWith("/entries") || pathname.startsWith("/visualize")
}

export async function proxy(request: NextRequest) {
	const pathname = request.nextUrl.pathname
	const isMarkdown = acceptsMarkdown(request.headers.get("accept"))

	if (isMarkdown) {
		if (pathname === "/") {
			const url = request.nextUrl.clone()
			url.pathname = "/startpage.md"
			return NextResponse.rewrite(url)
		}

		if (pathname === "/start-guide") {
			const url = request.nextUrl.clone()
			url.pathname = "/start-guide.md"
			return NextResponse.rewrite(url)
		}

		if (pathname === "/mcp-setup") {
			const url = request.nextUrl.clone()
			url.pathname = "/mcp-setup.md"
			return NextResponse.rewrite(url)
		}
	}

	if (isProtectedAppRoute(pathname)) {
		const session = await readSessionFromRequest(request)
		if (!session) {
			const url = request.nextUrl.clone()
			url.pathname = "/api/auth/login"
			url.searchParams.set("returnTo", `${pathname}${request.nextUrl.search}`)
			return NextResponse.redirect(url)
		}
	}

	return NextResponse.next()
}

export const config = {
	matcher: ["/", "/entries/:path*", "/visualize", "/visualize/:path*", "/start-guide", "/mcp-setup"],
}

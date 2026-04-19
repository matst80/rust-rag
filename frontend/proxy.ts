import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { acceptsMarkdown } from "@/lib/start-page"

export function proxy(request: NextRequest) {
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

	return NextResponse.next()
}

export const config = {
	matcher: ["/", "/start-guide", "/mcp-setup"],
}

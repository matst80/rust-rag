import type { NextRequest } from "next/server"
import { NextResponse } from "next/server"
import { acceptsMarkdown } from "@/lib/start-page"

export function proxy(request: NextRequest) {
	if (
		request.nextUrl.pathname === "/" &&
		acceptsMarkdown(request.headers.get("accept"))
	) {
		const markdownUrl = request.nextUrl.clone()
		markdownUrl.pathname = "/startpage-markdown"
		return NextResponse.rewrite(markdownUrl)
	}

	return NextResponse.next()
}

export const config = {
	matcher: ["/"],
}

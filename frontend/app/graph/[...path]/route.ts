import type { NextRequest } from "next/server"
import { proxyRagRequest } from "@/lib/rag-proxy"

export const runtime = "nodejs"

async function resolveBackendPath(paramsPromise: Promise<{ path: string[] }>) {
	const { path } = await paramsPromise
	return `/graph/${path.join("/")}`
}

export async function GET(
	request: NextRequest,
	context: { params: Promise<{ path: string[] }> }
) {
	return proxyRagRequest(request, await resolveBackendPath(context.params))
}

export async function POST(
	request: NextRequest,
	context: { params: Promise<{ path: string[] }> }
) {
	return proxyRagRequest(request, await resolveBackendPath(context.params))
}

export async function DELETE(
	request: NextRequest,
	context: { params: Promise<{ path: string[] }> }
) {
	return proxyRagRequest(request, await resolveBackendPath(context.params))
}
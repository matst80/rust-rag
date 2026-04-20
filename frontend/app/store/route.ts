import type { NextRequest } from "next/server"
import { proxyRagRequest } from "@/lib/rag-proxy"

export const runtime = "nodejs"

export async function POST(request: NextRequest) {
	return proxyRagRequest(request, "/store")
}
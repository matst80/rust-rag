import { jwtVerify, SignJWT } from "jose"
import type { NextRequest, NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"

export const SESSION_COOKIE_NAME = "rag_session"

export interface AuthSession {
	sub: string
	name?: string
	email?: string
	preferred_username?: string
	exp: number
}

function getSessionKey() {
	return new TextEncoder().encode(getAuthConfig().sessionSecret)
}

function isSecureCookie() {
	return process.env.NODE_ENV === "production"
}

export async function createSessionToken(
	claims: Omit<AuthSession, "exp">,
	maxAgeSeconds: number
): Promise<string> {
	return new SignJWT({
		email: claims.email,
		name: claims.name,
		preferred_username: claims.preferred_username,
	})
		.setProtectedHeader({ alg: "HS256" })
		.setSubject(claims.sub)
		.setIssuedAt()
		.setExpirationTime(`${maxAgeSeconds}s`)
		.sign(getSessionKey())
}

export async function verifySessionToken(token: string): Promise<AuthSession | null> {
	try {
		const { payload } = await jwtVerify(token, getSessionKey())
		if (typeof payload.sub !== "string" || typeof payload.exp !== "number") {
			return null
		}

		return {
			sub: payload.sub,
			exp: payload.exp,
			name: typeof payload.name === "string" ? payload.name : undefined,
			email: typeof payload.email === "string" ? payload.email : undefined,
			preferred_username:
				typeof payload.preferred_username === "string"
					? payload.preferred_username
					: undefined,
		}
	} catch {
		return null
	}
}

export async function readSessionFromRequest(request: NextRequest): Promise<AuthSession | null> {
	const token = request.cookies.get(SESSION_COOKIE_NAME)?.value
	if (!token) {
		return null
	}

	return verifySessionToken(token)
}

export async function setSessionCookie(
	response: NextResponse,
	claims: Omit<AuthSession, "exp">,
	maxAgeSeconds: number
) {
	const token = await createSessionToken(claims, maxAgeSeconds)
	response.cookies.set({
		name: SESSION_COOKIE_NAME,
		value: token,
		httpOnly: true,
		sameSite: "lax",
		secure: isSecureCookie(),
		path: "/",
		maxAge: maxAgeSeconds,
	})
	return token
}

export function clearSessionCookie(response: NextResponse) {
	response.cookies.set({
		name: SESSION_COOKIE_NAME,
		value: "",
		httpOnly: true,
		sameSite: "lax",
		secure: isSecureCookie(),
		path: "/",
		maxAge: 0,
	})
}
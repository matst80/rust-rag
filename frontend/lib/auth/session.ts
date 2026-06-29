import { EncryptJWT, jwtDecrypt, jwtVerify, SignJWT } from "jose"
import { cookies } from "next/headers"
import type { NextRequest, NextResponse } from "next/server"
import { getAuthConfig } from "@/lib/auth/config"

export const SESSION_COOKIE_NAME = "rag_session"
export const REFRESH_COOKIE_NAME = "rag_refresh"

export interface UserSession {
	sub: string
	name?: string
	email?: string
	preferred_username?: string
}

export interface RefreshPayload {
	refresh_token: string
	user: UserSession
}

function getSessionSecret() {
	const config = getAuthConfig()
	return new TextEncoder().encode(config.sessionSecret)
}

async function getRefreshKey(): Promise<Uint8Array> {
	const config = getAuthConfig()
	const data = new TextEncoder().encode(config.sessionSecret)
	const hash = await crypto.subtle.digest("SHA-256", data)
	return new Uint8Array(hash)
}

function isSecureCookie() {
	const config = getAuthConfig()
	return config.appBaseUrl.startsWith("https")
}

export async function createSessionToken(user: UserSession, maxAge: number): Promise<string> {
	const secret = getSessionSecret()

	return new SignJWT({ ...user })
		.setProtectedHeader({ alg: "HS256" })
		.setIssuedAt()
		.setExpirationTime(Math.floor(Date.now() / 1000) + maxAge)
		.sign(secret)
}

export async function readSessionFromRequest(request: NextRequest): Promise<UserSession | null> {
	const cookie = request.cookies.get(SESSION_COOKIE_NAME)
	return verifySessionCookieValue(cookie?.value)
}

export async function readSessionFromCookies(): Promise<UserSession | null> {
	const cookieStore = await cookies()
	const cookie = cookieStore.get(SESSION_COOKIE_NAME)
	return verifySessionCookieValue(cookie?.value)
}

async function verifySessionCookieValue(value: string | undefined): Promise<UserSession | null> {
	if (!value) {
		return null
	}

	try {
		const secret = getSessionSecret()
		const { payload } = await jwtVerify(value, secret, {
			algorithms: ["HS256"],
		})

		return payload as unknown as UserSession
	} catch (error) {
		console.error("session verification failed", error)
		return null
	}
}

export async function setSessionCookie(response: NextResponse, user: UserSession, maxAge: number) {
	const token = await createSessionToken(user, maxAge)

	response.cookies.set({
		name: SESSION_COOKIE_NAME,
		value: token,
		httpOnly: true,
		sameSite: "lax",
		secure: isSecureCookie(),
		path: "/",
		maxAge,
	})
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

export async function createRefreshToken(payload: RefreshPayload, maxAge: number): Promise<string> {
	const key = await getRefreshKey()
	return new EncryptJWT({ ...payload })
		.setProtectedHeader({ alg: "dir", enc: "A256GCM" })
		.setIssuedAt()
		.setExpirationTime(Math.floor(Date.now() / 1000) + maxAge)
		.encrypt(key)
}

export async function readRefreshCookie(value: string | undefined): Promise<RefreshPayload | null> {
	if (!value) {
		return null
	}
	try {
		const key = await getRefreshKey()
		const { payload } = await jwtDecrypt(value, key)
		const data = payload as unknown as RefreshPayload
		if (!data.refresh_token || !data.user?.sub) {
			return null
		}
		return data
	} catch (error) {
		console.error("refresh cookie decrypt failed", error)
		return null
	}
}

export async function readRefreshFromRequest(request: NextRequest): Promise<RefreshPayload | null> {
	return readRefreshCookie(request.cookies.get(REFRESH_COOKIE_NAME)?.value)
}

export async function setRefreshCookie(
	response: NextResponse,
	payload: RefreshPayload,
	maxAge: number
) {
	const token = await createRefreshToken(payload, maxAge)
	response.cookies.set({
		name: REFRESH_COOKIE_NAME,
		value: token,
		httpOnly: true,
		sameSite: "lax",
		secure: isSecureCookie(),
		path: "/",
		maxAge,
	})
}

export function clearRefreshCookie(response: NextResponse) {
	response.cookies.set({
		name: REFRESH_COOKIE_NAME,
		value: "",
		httpOnly: true,
		sameSite: "lax",
		secure: isSecureCookie(),
		path: "/",
		maxAge: 0,
	})
}

const DEFAULT_SCOPES = "openid profile email"
const DEFAULT_SESSION_MAX_AGE_SECS = 8 * 60 * 60

export interface AuthConfig {
	appBaseUrl: string
	backendApiUrl: string
	backendApiKey?: string
	issuer: string
	clientId: string
	clientSecret: string
	redirectUri: string
	scopes: string
	sessionSecret: string
	sessionMaxAgeSeconds: number
}

function requiredEnv(name: string): string {
	const value = process.env[name]?.trim()
	if (!value) {
		throw new Error(`Missing required environment variable ${name}`)
	}

	return value
}

function optionalEnv(name: string): string | undefined {
	const value = process.env[name]?.trim()
	return value ? value : undefined
}

export function getAuthConfig(): AuthConfig {
	const appBaseUrl = requiredEnv("APP_BASE_URL")
	const sessionMaxAgeSeconds = Number.parseInt(
		process.env.AUTH_SESSION_MAX_AGE_SECS ?? `${DEFAULT_SESSION_MAX_AGE_SECS}`,
		10
	)

	return {
		appBaseUrl,
		backendApiUrl: optionalEnv("RAG_API_URL") ?? "http://127.0.0.1:4001",
		backendApiKey: optionalEnv("RAG_FRONTEND_API_KEY"),
		issuer: requiredEnv("ZITADEL_ISSUER"),
		clientId: requiredEnv("ZITADEL_CLIENT_ID"),
		clientSecret: requiredEnv("ZITADEL_CLIENT_SECRET"),
		redirectUri: optionalEnv("ZITADEL_REDIRECT_URI") ?? `${appBaseUrl}/api/auth/callback`,
		scopes: optionalEnv("ZITADEL_SCOPES") ?? DEFAULT_SCOPES,
		sessionSecret: requiredEnv("AUTH_SESSION_SECRET"),
		sessionMaxAgeSeconds:
			Number.isFinite(sessionMaxAgeSeconds) && sessionMaxAgeSeconds > 0
				? sessionMaxAgeSeconds
				: DEFAULT_SESSION_MAX_AGE_SECS,
	}
}
// Typed access to the OtelRUM instance loaded in app/layout.tsx. The RUM
// agent must run before any app code, so it is injected as a beforeInteractive
// script and read off `window.OtelRUM` here instead of being imported.
//
// Backend issue → frontend issue correlation: `traceHeaders()` propagates the
// RUM trace context onto API fetches so frontend requests connect to backend
// traces in the collector.

interface OtelRumInstance {
  traceHeaders?: () => Record<string, string>
  captureException?: (error: unknown, attributes?: Record<string, unknown>) => void
}

interface OtelRumGlobal {
  last?: OtelRumInstance
}

export function rumInstance(): OtelRumInstance | undefined {
  if (typeof window === "undefined") return undefined
  return (window as unknown as { OtelRUM?: OtelRumGlobal }).OtelRUM?.last
}

export function rumTraceHeaders(): Record<string, string> {
  try {
    const headers = rumInstance()?.traceHeaders?.()
    if (headers && typeof headers === "object") return headers
  } catch {
    // RUM not loaded (blocked, SSR, old bundle) — requests just go out bare.
  }
  return {}
}

export function rumCaptureException(
  error: unknown,
  attributes?: Record<string, unknown>
): void {
  try {
    rumInstance()?.captureException?.(error, attributes)
  } catch {
    // Never let reporting break error handling.
  }
}

import type { Metadata } from 'next'
import Script from 'next/script'
import { Geist, JetBrains_Mono, Source_Serif_4 } from 'next/font/google'
import { Providers } from '@/components/providers'
import { AppShell } from '@/components/app-shell'
import AppErrorBoundary from '@/components/app-error-boundary'
import './globals.css'

// RUM identity — the release is baked at build time (NEXT_PUBLIC_APP_RELEASE)
// and must exactly match the source-map upload release so error stacks are
// de-obfuscated (https://otel.k6n.net/instrumentation.md).
const RUM_BASE_URL = process.env.NEXT_PUBLIC_RUM_BASE_URL || 'https://otel.k6n.net'
const RUM_SERVICE_NAME = process.env.NEXT_PUBLIC_RUM_SERVICE_NAME || 'rust-rag-web'
const RUM_RELEASE = process.env.NEXT_PUBLIC_APP_RELEASE || 'dev'
const RUM_ENVIRONMENT = process.env.NEXT_PUBLIC_APP_ENV || 'production'

const rumInitScript = `
  window.OtelRUM = window.OtelRUM || {};
  if (typeof OtelRUM !== 'undefined' && typeof OtelRUM.init === 'function') {
    window.OtelRUM.last = OtelRUM.init({
      serviceName: ${JSON.stringify(RUM_SERVICE_NAME)},
      release: ${JSON.stringify(RUM_RELEASE)},
      environment: ${JSON.stringify(RUM_ENVIRONMENT)},
      baseUrl: ${JSON.stringify(RUM_BASE_URL)},
    });
  }
`

const _geist = Geist({ subsets: ["latin"] })
const _jetbrainsMono = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-jetbrains-mono",
})
const _sourceSerif = Source_Serif_4({
  subsets: ["latin"],
  variable: "--font-source-serif",
})

export const metadata: Metadata = {
  title: 'RAG Memory & Knowledge',
  description: 'Search and manage your RAG memory and knowledge base',
  generator: 'v0.app',
  manifest: '/manifest.json',
  icons: {
    icon: [
      {
        url: '/icon.svg',
        type: 'image/svg+xml',
      },
    ],
    apple: '/apple-icon.png',
  },
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className={`${_jetbrainsMono.variable} ${_sourceSerif.variable} font-sans antialiased bg-background`}>
        {/* RUM must load before any app code so it can catch early errors. */}
        <Script src={`${RUM_BASE_URL}/static/otel-rum.js`} strategy="beforeInteractive" />
        <Script id="otel-rum-init" strategy="beforeInteractive" dangerouslySetInnerHTML={{ __html: rumInitScript }} />
        <Providers>
          <AppShell>
            <AppErrorBoundary>
              {children}
            </AppErrorBoundary>
          </AppShell>
        </Providers>
        <script
          dangerouslySetInnerHTML={{
            __html: `
              if ('serviceWorker' in navigator) {
                window.addEventListener('load', function() {
                  navigator.serviceWorker.register('/sw.js');
                });
              }
            `,
          }}
        />
      </body>
    </html>
  )
}

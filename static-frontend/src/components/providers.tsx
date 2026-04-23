import { ThemeProvider } from "next-themes"
import { SWRConfig } from "swr"

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem={true}>
      <SWRConfig
        value={{
          revalidateOnFocus: false,
          errorRetryCount: 2,
        }}
      >
        {children}
      </SWRConfig>
    </ThemeProvider>
  )
}

"use client"

import React from "react"
import { Button } from "@/components/ui/button"
import { rumCaptureException } from "@/lib/rum"

interface AppErrorBoundaryProps {
  children: React.ReactNode
}

interface AppErrorBoundaryState {
  error: Error | null
}

// Global handlers miss React render errors, so this boundary reports them to
// RUM (see https://otel.k6n.net/instrumentation.md §3) and renders a recovery
// UI. Wrap page content, not the app shell, so navigation stays usable.
class AppErrorBoundary extends React.Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  constructor(props: AppErrorBoundaryProps) {
    super(props)
    this.state = { error: null }
  }

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error }
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    rumCaptureException(error, {
      "exception.type": "ReactErrorBoundary",
      "react.component_stack": info.componentStack,
    })
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex flex-col items-center justify-center gap-4 p-12 text-center">
          <h2 className="text-lg font-semibold">Something went wrong</h2>
          <p className="max-w-md text-sm text-muted-foreground">
            {this.state.error.message || "An unexpected error occurred while rendering this view."}
          </p>
          <Button variant="outline" onClick={() => this.setState({ error: null })}>
            Try again
          </Button>
        </div>
      )
    }

    return this.props.children
  }
}

export default AppErrorBoundary

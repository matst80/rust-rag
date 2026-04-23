"use client"

import * as React from "react"
import { Trash2, X, Loader2, Check } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

export interface ComboButtonProps extends Omit<React.ComponentProps<typeof Button>, "onClick"> {
  onConfirm: () => Promise<void> | void
  idleIcon?: React.ReactNode
  confirmLabel?: string
  cancelLabel?: string
  successLabel?: string
}

export function ComboButton({
  onConfirm,
  idleIcon = <Trash2 className="size-4" />,
  confirmLabel = "Delete",
  cancelLabel = "Cancel",
  successLabel,
  className,
  variant = "ghost",
  size = "icon",
  ...props
}: ComboButtonProps) {
  const [state, setState] = React.useState<"idle" | "confirming" | "loading" | "success">("idle")

  const handleInitialClick = (e: React.MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setState("confirming")
  }

  const handleCancel = (e: React.MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setState("idle")
  }

  const handleConfirm = async (e: React.MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setState("loading")
    try {
      await onConfirm()
      setState("success")
      // Reset after 2 seconds if still mounted
      setTimeout(() => {
        if (state === "success") setState("idle")
      }, 2000)
    } catch (error) {
      setState("idle")
    }
  }

  // Effect to handle clicking outside to cancel
  React.useEffect(() => {
    if (state !== "confirming") return

    const handleClickOutside = () => setState("idle")
    window.addEventListener("click", handleClickOutside)
    return () => window.removeEventListener("click", handleClickOutside)
  }, [state])

  if (state === "confirming") {
    return (
      <div 
        className={cn(
          "flex items-center gap-2 p-1.5 bg-background/80 backdrop-blur-md rounded-full border border-destructive/20 animate-in fade-in zoom-in-95 duration-200 shadow-xl shadow-destructive/10 w-auto h-auto",
          className && className.split(' ').filter(c => !c.startsWith('size-') && !c.startsWith('w-') && !c.startsWith('h-')).join(' ')
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <Button
          variant="destructive"
          size="sm"
          className="h-8 px-4 text-[10px] font-black uppercase tracking-widest rounded-full shadow-lg shadow-destructive/20 transition-all active:scale-95 py-0 animate-pulse"
          onClick={handleConfirm}
        >
          {confirmLabel}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-8 px-3 rounded-full text-muted-foreground hover:bg-muted/50 hover:text-foreground text-[10px] font-bold uppercase tracking-widest transition-colors"
          onClick={handleCancel}
        >
          {cancelLabel}
        </Button>
      </div>
    )
  }

  return (
    <Button
      variant={state === "success" ? "outline" : variant}
      size={size}
      className={cn(
        "transition-all duration-300 relative",
        state === "loading" && "pointer-events-none",
        state === "success" && "border-green-500/50 text-green-500 bg-green-500/5 shadow-none",
        state === "idle" && "text-muted-foreground/60 hover:text-destructive hover:bg-destructive/5",
        className
      )}
      onClick={handleInitialClick}
      title={state === "idle" ? "Delete" : undefined}
      {...props}
    >
      <div className={cn(
        "flex items-center justify-center transition-all duration-300 w-full h-full",
        state === "loading" ? "scale-90" : "scale-100"
      )}>
        {state === "loading" ? (
          <Loader2 className="size-4 animate-spin text-destructive" />
        ) : state === "success" ? (
          successLabel ? <span className="text-[10px] font-bold uppercase px-2">{successLabel}</span> : <Check className="size-4" />
        ) : (
          idleIcon
        )}
      </div>
    </Button>
  )
}

"use client"

import { useEffect, useRef, memo } from "react"
import { Terminal } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import "@xterm/xterm/css/xterm.css"

interface TerminalViewProps {
	terminalId: string
	onInput: (data: string) => void
	onResize: (cols: number, rows: number) => void
	// Event emitter for terminal output from parent
	output?: { data: string; ts: number }
	snapshot?: { data: string; cols: number; rows: number; ts: number }
}

export const TerminalView = memo(function TerminalView({
	terminalId,
	onInput,
	onResize,
	output,
	snapshot,
}: TerminalViewProps) {
	const containerRef = useRef<HTMLDivElement>(null)
	const termRef = useRef<Terminal | null>(null)
	const fitAddonRef = useRef<FitAddon | null>(null)
	const lastOutputTs = useRef(0)
	const lastSnapshotTs = useRef(0)

	useEffect(() => {
		if (!containerRef.current) return

		const term = new Terminal({
			cursorBlink: true,
			fontSize: 12,
			fontFamily: 'var(--font-mono), ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
			theme: {
				background: "#09090b", // zinc-950
				foreground: "#fafafa", // zinc-50
			},
		})

		const fitAddon = new FitAddon()
		term.loadAddon(fitAddon)

		term.open(containerRef.current)
		fitAddon.fit()

		term.onData((data) => {
			const bytes = new TextEncoder().encode(data)
			let binary = ""
			for (let i = 0; i < bytes.length; i++) {
				binary += String.fromCharCode(bytes[i])
			}
			const base64 = btoa(binary)
			onInput(base64)
		})

		term.onResize(({ cols, rows }) => {
			onResize(cols, rows)
		})

		termRef.current = term
		fitAddonRef.current = fitAddon

		const handleResize = () => {
			fitAddon.fit()
		}
		window.addEventListener("resize", handleResize)

		// Initial resize
		const dims = fitAddon.proposeDimensions()
		if (dims) {
			onResize(dims.cols, dims.rows)
		}

		return () => {
			window.removeEventListener("resize", handleResize)
			term.dispose()
		}
	}, [terminalId, onInput, onResize])

	// Handle snapshot
	useEffect(() => {
		if (!termRef.current || !snapshot || snapshot.ts <= lastSnapshotTs.current) return
		lastSnapshotTs.current = snapshot.ts
		
		const term = termRef.current
		const binary = atob(snapshot.data)
		const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0))
		const decoded = new TextDecoder().decode(bytes)
		
		term.reset()
		if (snapshot.cols && snapshot.rows) {
			term.resize(snapshot.cols, snapshot.rows)
		}
		term.write(decoded)
		
		// If fitAddon exists, refit after snapshot
		fitAddonRef.current?.fit()
	}, [snapshot])

	// Handle incremental output
	useEffect(() => {
		if (!termRef.current || !output || output.ts <= lastOutputTs.current) return
		lastOutputTs.current = output.ts
		
		const binary = atob(output.data)
		const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0))
		const decoded = new TextDecoder().decode(bytes)
		termRef.current.write(decoded)
	}, [output])

	return (
		<div className="w-full h-full bg-zinc-950 p-2 overflow-hidden rounded-md border border-border/50 shadow-inner">
			<div ref={containerRef} className="w-full h-full" />
		</div>
	)
})

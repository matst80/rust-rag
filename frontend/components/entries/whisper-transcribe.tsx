"use client"

import { useState, useRef, useEffect, useMemo } from "react"
import { Mic, MicOff, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"
import { useItem } from "@/lib/api/hooks"

interface WhisperTranscribeProps {
  onTranscription: (text: string) => void
}

export function WhisperTranscribe({ onTranscription }: WhisperTranscribeProps) {
  const [isRecording, setIsRecording] = useState(false)
  const [isConnecting, setIsConnecting] = useState(false)
  const socketRef = useRef<WebSocket | null>(null)
  const audioContextRef = useRef<AudioContext | null>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const processorRef = useRef<ScriptProcessorNode | null>(null)

  // Fetch the whisper API entry and overview to find the URL
  // The user specifically requested to enable this RAG ID
  const { data: whisperApiEntry } = useItem("whisper_slask_websocket_api_v1")
  const { data: whisperOverviewEntry } = useItem("projects_whisper_slask_overview")

  const whisperWsUrl = useMemo(() => {
    // 1. Try to find a ws:// or wss:// URL in the overview or api entry (manual override)
    const urlRegex = /ws:\/\/[\d.]+(?::\d+)?\/ws/g
    const overviewMatch = whisperOverviewEntry?.text.match(urlRegex)
    if (overviewMatch) return overviewMatch[0]
    
    const apiMatch = whisperApiEntry?.text.match(urlRegex)
    if (apiMatch) return apiMatch[0]
    
    // 2. Use the backend proxy (relative to the current host)
    if (typeof window !== "undefined") {
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:"
      const host = window.location.host
      
      // In development, the backend might be on 4001 while frontend is on 3000
      // If host is localhost:3000, we'll try to guess if we should use 4001 directly
      // as Next.js rewrites don't always support WebSocket upgrades.
      if (host.includes(":3000")) {
        return `${protocol}//${host.replace(":3000", ":4001")}/api/whisper/ws`
      }
      
      return `${protocol}//${host}/api/whisper/ws`
    }
    
    // Final fallback
    return "ws://10.10.3.30:80/ws" 
  }, [whisperApiEntry, whisperOverviewEntry])

  const stopRecording = () => {
    setIsRecording(false)
    setIsConnecting(false)

    if (processorRef.current) {
      processorRef.current.disconnect()
      processorRef.current = null
    }

    if (audioContextRef.current) {
      audioContextRef.current.close()
      audioContextRef.current = null
    }

    if (streamRef.current) {
      streamRef.current.getTracks().forEach(track => track.stop())
      streamRef.current = null
    }

    if (socketRef.current) {
      socketRef.current.close()
      socketRef.current = null
    }
  }

  const startRecording = async () => {
    try {
      setIsConnecting(true)
      
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      streamRef.current = stream

      console.log(`Connecting to Whisper at ${whisperWsUrl}...`)
      const socket = new WebSocket(whisperWsUrl)
      socketRef.current = socket

      socket.onopen = () => {
        setIsConnecting(false)
        setIsRecording(true)
        
        const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)({
          sampleRate: 16000,
        })
        audioContextRef.current = audioContext

        const source = audioContext.createMediaStreamSource(stream)
        // ScriptProcessorNode is deprecated but widely supported for simple cases
        // 4096 is a good buffer size for 16kHz
        const processor = audioContext.createScriptProcessor(4096, 1, 1)
        processorRef.current = processor

        processor.onaudioprocess = (e) => {
          if (socket.readyState === WebSocket.OPEN) {
            const inputData = e.inputBuffer.getChannelData(0)
            socket.send(inputData.buffer)
          }
        }

        source.connect(processor)
        processor.connect(audioContext.destination)
      }

      socket.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data)
          if (data.type === "Transcription" && data.text) {
            onTranscription(data.text)
          }
        } catch (e) {
          console.error("Failed to parse whisper message", e)
        }
      }

      socket.onerror = (error) => {
        console.error("Whisper WebSocket error", error)
        stopRecording()
      }

      socket.onclose = () => {
        stopRecording()
      }

    } catch (err) {
      console.error("Failed to start recording", err)
      stopRecording()
    }
  }

  const toggleRecording = () => {
    if (isRecording || isConnecting) {
      stopRecording()
    } else {
      startRecording()
    }
  }

  useEffect(() => {
    return () => {
      stopRecording()
    }
  }, [])

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant={isRecording ? "destructive" : "outline"}
            size="sm"
            className="gap-2 h-8 px-2 transition-all duration-300"
            onClick={toggleRecording}
            disabled={isConnecting}
          >
            {isConnecting ? (
              <Loader2 className="size-4 animate-spin text-primary" />
            ) : isRecording ? (
              <MicOff className="size-4 animate-pulse" />
            ) : (
              <Mic className="size-4" />
            )}
            <span className="text-xs font-medium">
              {isConnecting ? "Connecting..." : isRecording ? "Stop Rec" : "Transcribe"}
            </span>
          </Button>
        </TooltipTrigger>
        <TooltipContent side="top">
          <p className="text-xs">
            {isRecording ? "Stop recording" : `Transcription via ${whisperWsUrl}`}
          </p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

import { useState, useRef, useEffect } from 'react';
import type { Config, ChatMessage } from '../types';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface Props {
  config: Config;
}

interface StreamingState {
  content: string;
  thinking: string;
}

export function ChatTab({ config }: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [streaming, setStreaming] = useState<StreamingState | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const scrollToBottom = () => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  };

  useEffect(scrollToBottom, [messages, streaming]);

  const send = async () => {
    const text = input.trim();
    if (!text || streaming) return;

    setInput('');
    setMessages((prev) => [...prev, { id: Date.now().toString(), role: 'user', content: text, thinking: '' }]);

    setStreaming({ content: '', thinking: '' });
    let accContent = '';
    let accThinking = '';

    try {
      const res = await fetch(`${config.apiBaseUrl}/api/openai/v1/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {}),
        },
        body: JSON.stringify({
          messages: [{ role: 'user', content: text }],
          stream: true,
        }),
      });

      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);

      const reader = res.body!.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const parts = buffer.split('\n\n');
        buffer = parts.pop() ?? '';

        for (const part of parts) {
          const dataLine = part
            .split('\n')
            .filter((l) => l.trim().startsWith('data:'))
            .map((l) => l.trim().slice(5).trim())
            .join('\n');

          if (!dataLine || dataLine === '[DONE]') continue;

          try {
            const parsed = JSON.parse(dataLine) as {
              choices?: Array<{ delta?: { content?: string; reasoning_content?: string; reasoning?: string } }>;
            };
            const delta = parsed.choices?.[0]?.delta;
            if (!delta) continue;

            const thinking = delta.reasoning_content ?? delta.reasoning;
            if (thinking) accThinking += thinking;
            if (delta.content) accContent += delta.content;

            setStreaming({ content: accContent, thinking: accThinking });
          } catch {
            // malformed SSE chunk — skip
          }
        }
      }
    } catch (e) {
      accContent += `\n[Error: ${e instanceof Error ? e.message : 'Unknown error'}]`;
    }

    setMessages((prev) => [
      ...prev,
      { id: Date.now().toString(), role: 'ai', content: accContent, thinking: accThinking },
    ]);
    setStreaming(null);
    inputRef.current?.focus();
  };

  return (
    <>
      <div ref={listRef} className="chat-messages">
        {messages.length === 0 && !streaming && (
          <p className="placeholder">Ask anything about<br />your knowledge base</p>
        )}
        {messages.map((msg) => (
          <MessageBubble key={msg.id} msg={msg} />
        ))}
        {streaming && (
          <div className="chat-msg ai">
            <div className="msg-label">AI</div>
            {streaming.thinking && (
              <div className="thinking-block">{streaming.thinking}</div>
            )}
            <div className="msg-content">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{streaming.content}</ReactMarkdown>
              <span className="cursor" />
            </div>
          </div>
        )}
      </div>
      <div className="search-box">
        <input
          ref={inputRef}
          type="text"
          placeholder="Ask a question..."
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && send()}
          disabled={!!streaming}
        />
        <button onClick={send} disabled={!!streaming}>
          {streaming ? (
            <div className="spinner" />
          ) : (
            <svg viewBox="0 0 24 24" width="17" height="17">
              <path fill="currentColor" d="M2,21L23,12L2,3V10L17,12L2,14V21Z" />
            </svg>
          )}
        </button>
      </div>
    </>
  );
}

function MessageBubble({ msg }: { msg: ChatMessage }) {
  return (
    <div className={`chat-msg ${msg.role}`}>
      <div className="msg-label">{msg.role === 'user' ? 'You' : 'AI'}</div>
      {msg.thinking && <div className="thinking-block">{msg.thinking}</div>}
      <div className="msg-content">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{msg.content}</ReactMarkdown>
      </div>
    </div>
  );
}

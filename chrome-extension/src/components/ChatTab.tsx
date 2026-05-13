import { useState, useRef, useEffect } from 'react';
import type { Config, ChatMessage } from '../types';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { usePageContent } from '../hooks/usePageContent';

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
  const { content: pageContent, refreshContent } = usePageContent();

  useEffect(() => {
    refreshContent();
  }, [refreshContent]);

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
      const userMessages = messages.map(m => ({ role: m.role === 'user' ? 'user' : 'assistant', content: m.content }));
      const currentMessages = [...userMessages];
      
      if (pageContent && messages.length === 0) {
        currentMessages.push({ 
          role: 'user', 
          content: `Context from current page:\n\n${pageContent.slice(0, 8000)}\n\n---\n\nUser Question: ${text}` 
        });
      } else {
        currentMessages.push({ role: 'user', content: text });
      }

      const res = await fetch(`${config.apiBaseUrl}/api/openai/v1/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {}),
        },
        body: JSON.stringify({
          messages: currentMessages,
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
        {pageContent && messages.length === 0 && (
          <div style={{ 
            fontSize: '9px', 
            fontFamily: 'var(--mono)', 
            color: 'var(--accent)', 
            opacity: 0.8, 
            padding: '4px 8px', 
            border: '1px dashed var(--accent-border)', 
            marginBottom: '8px',
            display: 'flex',
            justifyContent: 'center',
            alignItems: 'center',
            gap: '8px'
          }}>
            PAGE CONTEXT LOADED ({Math.round(pageContent.length / 1024)}KB)
            <button 
              onClick={(e) => { e.stopPropagation(); refreshContent(); }}
              style={{ background: 'none', border: 'none', color: 'inherit', cursor: 'pointer', padding: '2px', display: 'flex' }}
              title="Refresh page content"
            >
              <svg viewBox="0 0 24 24" width="10" height="10">
                <path fill="currentColor" d="M17.65,6.35C16.2,4.9 14.21,4 12,4A8,8 0 0,0 4,12A8,8 0 0,0 12,20C15.73,20 18.84,17.45 19.73,14H17.65C16.83,16.33 14.61,18 12,18A6,6 0 0,1 6,12A6,6 0 0,1 12,6C13.66,6 15.14,6.69 16.22,7.78L13,11H20V4L17.65,6.35Z" />
              </svg>
            </button>
          </div>
        )}
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

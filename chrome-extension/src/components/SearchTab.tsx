import { useState } from 'react';
import type { Config, SearchResult, AssistedResult, AssistedEvent } from '../types';
import { StartView } from './StartView';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface Props {
  config: Config;
}

function scoreColor(score: number): string {
  if (score >= 80) return 'var(--success)';
  if (score >= 50) return 'var(--accent)';
  return 'var(--text-2)';
}

function distanceToScore(distance: number): number {
  return Math.max(0, Math.round((1 - distance) * 100));
}

type Mode = 'normal' | 'assisted';
type State = 'idle' | 'loading' | 'done' | 'error';

export function SearchTab({ config }: Props) {
  const [query, setQuery] = useState('');
  const [mode, setMode] = useState<Mode>('normal');
  const [state, setState] = useState<State>('idle');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [assistedResults, setAssistedResults] = useState<AssistedResult[]>([]);
  const [subQueries, setSubQueries] = useState<string[]>([]);
  const [errorMsg, setErrorMsg] = useState('');

  const reset = () => {
    setResults([]);
    setAssistedResults([]);
    setSubQueries([]);
    setErrorMsg('');
  };

  const searchNormal = async (q: string) => {
    const res = await fetch(`${config.apiBaseUrl}/api/search`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {}),
      },
      body: JSON.stringify({ query: q, top_k: 5 }),
    });
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
    const data = (await res.json()) as { results: SearchResult[] };
    setResults(data.results ?? []);
  };

  const searchAssisted = async (q: string) => {
    const res = await fetch(`${config.apiBaseUrl}/api/query/assisted`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {}),
      },
      body: JSON.stringify({ query: q, top_k: 10 }),
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
          .join('');

        if (!dataLine || dataLine === '[DONE]') continue;

        try {
          const event = JSON.parse(dataLine) as AssistedEvent;
          if (event.object === 'assisted_query.queries') {
            setSubQueries(event.queries);
          } else if (event.object === 'assisted_query.merged') {
            setAssistedResults(event.results);
          }
        } catch {
          // skip malformed chunk
        }
      }
    }
  };

  const search = async () => {
    const q = query.trim();
    if (!q) return;

    setState('loading');
    reset();

    try {
      if (mode === 'normal') {
        await searchNormal(q);
      } else {
        await searchAssisted(q);
      }
      setState('done');
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : 'Search failed');
      setState('error');
    }
  };

  const isEmpty = state === 'done' && (
    mode === 'normal' ? results.length === 0 : assistedResults.length === 0
  );

  const detailUrl = (id: string) => `${config.apiBaseUrl}/entries/${encodeURIComponent(id)}`;

  return (
    <>
      <div className="search-box">
        <input
          type="text"
          placeholder={mode === 'assisted' ? 'Ask a question...' : 'Search knowledge base...'}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && search()}
          autoFocus
        />
        <button onClick={search} disabled={state === 'loading'}>
          {state === 'loading' ? (
            <div className="spinner" />
          ) : (
            <svg viewBox="0 0 24 24" width="17" height="17">
              <path fill="currentColor" d="M9.5,3A6.5,6.5 0 0,1 16,9.5C16,11.11 15.41,12.59 14.44,13.73L14.71,14H15.5L20.5,19L19,20.5L14,15.5V14.71L13.73,14.44C12.59,15.41 11.11,16 9.5,16A6.5,6.5 0 0,1 3,9.5A6.5,6.5 0 0,1 9.5,3M9.5,5C7,5 5,7 5,9.5C5,12 7,14 9.5,14C12,14 14,12 14,9.5C14,7 12,5 9.5,5Z" />
            </svg>
          )}
        </button>
      </div>

      <div className="mode-toggle">
        <button
          className={`mode-btn${mode === 'normal' ? ' active' : ''}`}
          onClick={() => { setMode('normal'); reset(); setState('idle'); }}
        >
          Vector
        </button>
        <button
          className={`mode-btn${mode === 'assisted' ? ' active' : ''}`}
          onClick={() => { setMode('assisted'); reset(); setState('idle'); }}
        >
          AI Assisted
        </button>
      </div>

      <div className="results-list">
        {state === 'idle' && (
          <StartView config={config} />
        )}
        {state === 'error' && (
          <p className="placeholder" style={{ color: 'var(--error)' }}>Error: {errorMsg}</p>
        )}
        {isEmpty && (
          <p className="placeholder">No results found</p>
        )}

        {/* Assisted: show sub-queries while streaming */}
        {mode === 'assisted' && subQueries.length > 0 && state === 'loading' && (
          <div className="sub-queries">
            {subQueries.map((sq, i) => (
              <div key={i} className="sub-query-pill">{sq}</div>
            ))}
          </div>
        )}

        {/* Normal results */}
        {mode === 'normal' && results.map((r, i) => {
          const score = r.score != null ? Math.round(r.score * 100) : null;
          const color = score != null ? scoreColor(score) : 'var(--text-2)';
          return (
            <a
              key={r.id}
              href={detailUrl(r.id)}
              target="_blank"
              rel="noreferrer"
              className="result-card"
              style={{ animationDelay: `${i * 40}ms` }}
            >
              <div className="result-text">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{r.text}</ReactMarkdown>
              </div>
              <div className="result-meta">
                <span className="result-source">{r.source_id}</span>
                {score != null && (
                  <div className="score-col">
                    <span className="score-label" style={{ color }}>{score}% match</span>
                    <div className="score-track">
                      <div className="score-fill" style={{ width: `${score}%`, background: color }} />
                    </div>
                  </div>
                )}
              </div>
            </a>
          );
        })}

        {/* Assisted results */}
        {mode === 'assisted' && assistedResults.map((r, i) => {
          const score = distanceToScore(r.distance);
          const color = scoreColor(score);
          return (
            <a
              key={r.id}
              href={detailUrl(r.id)}
              target="_blank"
              rel="noreferrer"
              className="result-card"
              style={{ animationDelay: `${i * 40}ms` }}
            >
              <div className="result-text">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{r.text}</ReactMarkdown>
              </div>
              <div className="result-meta">
                <span className="result-source">{r.source_id}</span>
                <div className="score-col">
                  <span className="score-label" style={{ color }}>{score}% match</span>
                  <div className="score-track">
                    <div className="score-fill" style={{ width: `${score}%`, background: color }} />
                  </div>
                </div>
              </div>
            </a>
          );
        })}
      </div>
    </>
  );
}

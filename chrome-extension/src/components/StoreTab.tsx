import { useState } from 'react';
import type { Config } from '../types';

interface Props {
  config: Config;
}

type StoreState = 'idle' | 'loading' | 'success' | 'error';

const SOURCES = [
  { value: 'chrome-extension', label: 'Chrome Extension' },
  { value: 'notes', label: 'Notes' },
  { value: 'knowledge', label: 'Knowledge' },
];

export function StoreTab({ config }: Props) {
  const [text, setText] = useState('');
  const [source, setSource] = useState('chrome-extension');
  const [storeState, setStoreState] = useState<StoreState>('idle');
  const [errorMsg, setErrorMsg] = useState('');

  const store = async () => {
    const trimmed = text.trim();
    if (!trimmed) return;

    setStoreState('loading');
    setErrorMsg('');

    try {
      const res = await fetch(`${config.apiBaseUrl}/api/store`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {}),
        },
        body: JSON.stringify({
          text: trimmed,
          source_id: source,
          metadata: { method: 'chrome-popup', stored_at: new Date().toISOString() },
        }),
      });

      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);

      setText('');
      setStoreState('success');
      setTimeout(() => setStoreState('idle'), 2500);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : 'Store failed');
      setStoreState('error');
      setTimeout(() => setStoreState('idle'), 3000);
    }
  };

  const btnLabel =
    storeState === 'loading' ? 'Storing...' :
    storeState === 'success' ? '✓ Stored' :
    'Store Entry';

  return (
    <>
      <textarea
        className="store-textarea"
        placeholder="Enter text to store in RAG..."
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      {storeState === 'error' && (
        <div className="error-msg">Error: {errorMsg}</div>
      )}
      <div className="store-footer">
        <select value={source} onChange={(e) => setSource(e.target.value)}>
          {SOURCES.map((s) => (
            <option key={s.value} value={s.value}>{s.label}</option>
          ))}
        </select>
        <button
          className="btn-primary"
          onClick={store}
          disabled={storeState === 'loading' || storeState === 'success'}
          style={storeState === 'success' ? { background: 'var(--success)' } : undefined}
        >
          {btnLabel}
        </button>
      </div>
    </>
  );
}

import { useState } from 'react';
import type { Config } from '../types';

interface Props {
  config: Config;
}

type StoreState = 'idle' | 'loading' | 'success' | 'error';

interface SmartResult {
  count: number;
  sources: string[];
}

const SOURCES = [
  { value: 'chrome-extension', label: 'Chrome Extension' },
  { value: 'notes', label: 'Notes' },
  { value: 'knowledge', label: 'Knowledge' },
];

export function StoreTab({ config }: Props) {
  const [text, setText] = useState('');
  const [source, setSource] = useState('chrome-extension');
  const [storeState, setStoreState] = useState<StoreState>('idle');
  const [smartState, setSmartState] = useState<StoreState>('idle');
  const [errorMsg, setErrorMsg] = useState('');
  const [smartResult, setSmartResult] = useState<SmartResult | null>(null);

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

  const smartSave = async () => {
    const trimmed = text.trim();
    if (!trimmed) return;

    setSmartState('loading');
    setSmartResult(null);
    setErrorMsg('');

    try {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      const context: { url?: string; title?: string } = {};
      if (tab?.url) context.url = tab.url;
      if (tab?.title) context.title = tab.title;

      const res = await fetch(`${config.apiBaseUrl}/api/store/smart`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {}),
        },
        body: JSON.stringify({
          text: trimmed,
          context: Object.keys(context).length > 0 ? context : undefined,
        }),
      });

      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);

      const data = await res.json() as { items: Array<{ source_id: string }> };
      const sources = [...new Set(data.items.map((i) => i.source_id))];
      setSmartResult({ count: data.items.length, sources });
      setText('');
      setSmartState('success');
      setTimeout(() => { setSmartState('idle'); setSmartResult(null); }, 4000);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : 'Smart save failed');
      setSmartState('error');
      setTimeout(() => setSmartState('idle'), 3000);
    }
  };

  const isLoading = storeState === 'loading' || smartState === 'loading';

  return (
    <>
      <textarea
        className="store-textarea"
        placeholder="Enter text to store in RAG..."
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      {(storeState === 'error' || smartState === 'error') && (
        <div className="error-msg">Error: {errorMsg}</div>
      )}
      {smartState === 'success' && smartResult && (
        <div className="success-msg" style={{ fontSize: 11, color: 'var(--success)' }}>
          Saved {smartResult.count} item{smartResult.count !== 1 ? 's' : ''} →{' '}
          {smartResult.sources.join(', ')}
        </div>
      )}
      <div className="store-footer">
        <select
          value={source}
          onChange={(e) => setSource(e.target.value)}
          disabled={isLoading}
        >
          {SOURCES.map((s) => (
            <option key={s.value} value={s.value}>{s.label}</option>
          ))}
        </select>
        <button
          className="btn-secondary"
          onClick={smartSave}
          disabled={isLoading || smartState === 'success'}
          title="Let the AI categorize and chunk the text automatically"
          style={smartState === 'success' ? { background: 'var(--success)', color: '#fff' } : undefined}
        >
          {smartState === 'loading' ? 'Analyzing...' : smartState === 'success' ? '✓ Smart saved' : 'Smart save'}
        </button>
        <button
          className="btn-primary"
          onClick={store}
          disabled={isLoading || storeState === 'success'}
          style={storeState === 'success' ? { background: 'var(--success)' } : undefined}
        >
          {storeState === 'loading' ? 'Storing...' : storeState === 'success' ? '✓ Stored' : 'Store'}
        </button>
      </div>
    </>
  );
}

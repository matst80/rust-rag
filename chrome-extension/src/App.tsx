import { useState } from 'react';
import type { Tab } from './types';
import { useConfig } from './hooks/useConfig';
import { useConnection } from './hooks/useConnection';
import { TabBar } from './components/TabBar';
import { SearchTab } from './components/SearchTab';
import { ChatTab } from './components/ChatTab';
import { StoreTab } from './components/StoreTab';
import { SettingsModal } from './components/SettingsModal';
import { StatusBar } from './components/StatusBar';

export function App() {
  const { config, saveConfig, clearToken, loaded } = useConfig();
  const { online, checkConnection } = useConnection(config, loaded);
  const [tab, setTab] = useState<Tab>('search');
  const [settingsOpen, setSettingsOpen] = useState(false);

  if (!loaded) {
    return (
      <div className="container" style={{ alignItems: 'center', justifyContent: 'center' }}>
        <div className="spinner" style={{ width: 16, height: 16 }} />
      </div>
    );
  }

  // No token — show auth gating screen
  if (!config.apiToken) {
    return (
      <div className="container">
        <div className="section" style={{ justifyContent: 'center', gap: 16 }}>
          <p style={{
            fontFamily: 'var(--mono)',
            fontSize: 10,
            fontWeight: 900,
            textTransform: 'uppercase',
            letterSpacing: 3,
            color: 'var(--accent)',
          }}>
            rust-rag
          </p>
          <p style={{ fontSize: 12, color: 'var(--text-2)', lineHeight: 1.6 }}>
            Configure your API URL and authorize this extension to get started.
          </p>
          <button className="btn-primary" onClick={() => setSettingsOpen(true)}>
            Open Settings
          </button>
        </div>
        <StatusBar online={online} apiBaseUrl={config.apiBaseUrl} onSettingsClick={() => setSettingsOpen(true)} />
        {settingsOpen && (
          <SettingsModal
            config={config}
            onSave={async (updates) => { await saveConfig(updates); checkConnection(); }}
            onClose={() => setSettingsOpen(false)}
          />
        )}
      </div>
    );
  }

  return (
    <div className="container">
      <TabBar active={tab} onChange={setTab} />

      <div className="section">
        {tab === 'search' && <SearchTab config={config} />}
        {tab === 'chat'   && <ChatTab config={config} />}
        {tab === 'store'  && <StoreTab config={config} />}
      </div>

      <StatusBar
        online={online}
        apiBaseUrl={config.apiBaseUrl}
        onSettingsClick={() => setSettingsOpen(true)}
      />

      {settingsOpen && (
        <SettingsModal
          config={config}
          onSave={async (updates) => {
            await saveConfig(updates);
            if (updates.apiToken === null) clearToken();
            checkConnection();
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  );
}

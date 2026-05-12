import { useState } from 'react'
import { useConfig } from './hooks/useConfig'
import { useConnection } from './hooks/useConnection'
import { SearchTab } from './components/SearchTab'
import { LocalChatTab } from './components/LocalChatTab'
import { SmartSaveTab } from './components/SmartSaveTab'
import { SettingsModal } from './components/SettingsModal'
import { StatusBar } from './components/StatusBar'

type SideTab = 'chat' | 'save' | 'search'

const TABS: { id: SideTab; label: string }[] = [
  { id: 'chat', label: 'Chat' },
  { id: 'save', label: 'Save page' },
  { id: 'search', label: 'Search' },
]

export function SidePanelApp() {
  const { config, saveConfig, clearToken, loaded } = useConfig()
  const { online, checkConnection } = useConnection(config, loaded)
  const [tab, setTab] = useState<SideTab>('chat')
  const [settingsOpen, setSettingsOpen] = useState(false)

  if (!loaded) {
    return (
      <div className="container" style={{ alignItems: 'center', justifyContent: 'center' }}>
        <div className="spinner" style={{ width: 16, height: 16 }} />
      </div>
    )
  }

  if (!config.apiToken) {
    return (
      <div className="container">
        <div className="section" style={{ justifyContent: 'center', gap: 16, padding: 24 }}>
          <p style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 900, textTransform: 'uppercase', letterSpacing: 3, color: 'var(--accent)' }}>
            rust-rag side panel
          </p>
          <p style={{ fontSize: 12, color: 'var(--text-2)', lineHeight: 1.6 }}>
            Configure your API URL and authorize the extension first.
          </p>
          <button className="btn-primary" onClick={() => setSettingsOpen(true)}>Open Settings</button>
        </div>
        <StatusBar online={online} apiBaseUrl={config.apiBaseUrl} onSettingsClick={() => setSettingsOpen(true)} />
        {settingsOpen && (
          <SettingsModal
            config={config}
            onSave={async (updates) => { await saveConfig(updates); checkConnection() }}
            onClose={() => setSettingsOpen(false)}
          />
        )}
      </div>
    )
  }

  return (
    <div className="container" style={{ height: '100vh', display: 'flex', flexDirection: 'column' }}>
      <header style={{ display: 'flex', borderBottom: '1px solid var(--border, #333)' }}>
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`tab-btn ${tab === t.id ? 'active' : ''}`}
            onClick={() => setTab(t.id)}
            style={{ flex: 1 }}
          >
            {t.label}
          </button>
        ))}
      </header>

      <div className="section" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        {tab === 'chat' && <LocalChatTab config={config} />}
        {tab === 'save' && <SmartSaveTab config={config} />}
        {tab === 'search' && <SearchTab config={config} />}
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
            await saveConfig(updates)
            if (updates.apiToken === null) clearToken()
            checkConnection()
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  )
}

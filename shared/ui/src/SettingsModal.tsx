import React, { useState, useEffect } from 'react';

export interface ConfigShape {
  apiBaseUrl: string;
  apiToken?: string | null;
}

export interface SettingsModalProps {
  config: ConfigShape;
  onSave: (updates: Partial<ConfigShape>) => void;
  onClose: () => void;
}

export function SettingsModal({ config, onSave, onClose }: SettingsModalProps) {
  const [url, setUrl] = useState(config.apiBaseUrl);
  const [token, setToken] = useState(config.apiToken ?? '');

  useEffect(() => {
    setUrl(config.apiBaseUrl);
    setToken(config.apiToken ?? '');
  }, [config]);

  const save = () => {
    onSave({
      apiBaseUrl: url.trim().replace(/\/$/, ''),
      apiToken: token.trim() || null,
    });
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal-panel">
        <div className="modal-title">Settings</div>

        <div className="field">
          <label>API Base URL</label>
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://rag.k6n.net"
          />
        </div>

        <div className="field">
          <label>API Token (manual)</label>
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="Optional — use Device Flow below"
          />
        </div>

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={save}>Save</button>
        </div>
      </div>
    </div>
  );
}

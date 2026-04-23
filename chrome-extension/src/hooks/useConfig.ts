import { useState, useEffect } from 'react';
import type { Config } from '../types';

const DEFAULT_URL = 'https://rag.k6n.net';

export function useConfig() {
  const [config, setConfig] = useState<Config>({ apiBaseUrl: DEFAULT_URL, apiToken: null });
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    chrome.storage.local.get(['apiBaseUrl', 'apiToken']).then((stored) => {
      const apiBaseUrl = (stored.apiBaseUrl as string) || DEFAULT_URL;
      if (!stored.apiBaseUrl) {
        chrome.storage.local.set({ apiBaseUrl });
      }
      setConfig({ apiBaseUrl, apiToken: (stored.apiToken as string) || null });
      setLoaded(true);
    });

    // Keep in sync when the background writes the token after device auth
    const listener = (changes: Record<string, chrome.storage.StorageChange>) => {
      if (changes.apiToken) {
        setConfig((prev) => ({ ...prev, apiToken: changes.apiToken.newValue as string | null }));
      }
      if (changes.apiBaseUrl) {
        setConfig((prev) => ({ ...prev, apiBaseUrl: changes.apiBaseUrl.newValue as string }));
      }
    };
    chrome.storage.local.onChanged.addListener(listener);
    return () => chrome.storage.local.onChanged.removeListener(listener);
  }, []);

  const saveConfig = async (updates: Partial<Config>) => {
    const next = { ...config, ...updates };
    await chrome.storage.local.set({ apiBaseUrl: next.apiBaseUrl, apiToken: next.apiToken });
    setConfig(next);
  };

  const clearToken = async () => {
    await chrome.storage.local.remove('apiToken');
    setConfig((prev) => ({ ...prev, apiToken: null }));
  };

  return { config, saveConfig, clearToken, loaded };
}

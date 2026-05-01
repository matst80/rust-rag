import { useState, useEffect, useCallback } from 'react';
import type { Config } from '../types';

export function useConnection(config: Config, loaded: boolean) {
  const [online, setOnline] = useState(false);

  const check = useCallback(async () => {
    try {
      const res = await fetch(`${config.apiBaseUrl}/api/graph/status`, {
        headers: config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {},
      });
      setOnline(res.ok);
    } catch {
      setOnline(false);
    }
  }, [config.apiBaseUrl, config.apiToken]);

  useEffect(() => {
    if (loaded) check();
  }, [check, loaded]);

  return { online, checkConnection: check };
}

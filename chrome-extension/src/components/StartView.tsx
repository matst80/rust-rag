import { useState, useEffect } from 'react';
import type { Config, SearchResult } from '../types';

interface Props {
  config: Config;
}

interface LargeItem extends SearchResult {
  char_count: number;
}

export function StartView({ config }: Props) {
  const [latest, setLatest] = useState<SearchResult[]>([]);
  const [oversized, setOversized] = useState<LargeItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchData() {
      try {
        const headers = {
          'Content-Type': 'application/json',
          ...(config.apiToken ? { Authorization: `Bearer ${config.apiToken}` } : {}),
        };

        const [latestRes, largeRes] = await Promise.all([
          fetch(`${config.apiBaseUrl}/admin/items?limit=10&sort_order=desc`, { headers }),
          fetch(`${config.apiBaseUrl}/admin/items/oversized?limit=3`, { headers }),
        ]);

        if (latestRes.ok) {
          const data = await latestRes.json();
          setLatest(data.items || []);
        }
        if (largeRes.ok) {
          const data = await largeRes.json();
          setOversized(data.items || []);
        }
      } catch (e) {
        console.error('Failed to fetch start view data', e);
      } finally {
        setLoading(false);
      }
    }

    fetchData();
  }, [config]);

  const detailUrl = (id: string) => `${config.apiBaseUrl}/entries/${encodeURIComponent(id)}`;

  if (loading) {
    return (
      <div className="start-view loading">
        <div className="spinner-large" />
      </div>
    );
  }

  return (
    <div className="start-view animate-in">
      {oversized.length > 0 && (
        <section className="start-section">
          <div className="section-header">
            <svg viewBox="0 0 24 24" width="14" height="14" className="icon-warning">
              <path fill="currentColor" d="M13,14H11V10H13M13,18H11V16H13M1,21H23L12,2L1,21Z" />
            </svg>
            <h3>Oversized Entries</h3>
          </div>
          <div className="compact-list">
            {oversized.map((item) => (
              <a key={item.id} href={detailUrl(item.id)} target="_blank" rel="noreferrer" className="compact-item oversized">
                <div className="item-main">
                  <span className="item-id">{item.id}</span>
                  <p className="item-text">{item.text}</p>
                </div>
                <div className="item-badge">{Math.round(item.text.length / 1000)}k chars</div>
              </a>
            ))}
          </div>
        </section>
      )}

      <section className="start-section">
        <div className="section-header">
          <svg viewBox="0 0 24 24" width="14" height="14" className="icon-latest">
            <path fill="currentColor" d="M12,20A8,8 0 0,0 20,12A8,8 0 0,0 12,4A8,8 0 0,0 4,12A8,8 0 0,0 12,20M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22C6.47,22 2,17.5 2,12A10,10 0 0,1 12,2M12.5,7V12.25L17,14.92L16.25,16.15L11,13V7H12.5Z" />
          </svg>
          <h3>Latest Entries</h3>
        </div>
        <div className="compact-list">
          {latest.map((item) => (
            <a key={item.id} href={detailUrl(item.id)} target="_blank" rel="noreferrer" className="compact-item">
              <div className="item-main">
                <span className="item-source">{item.source_id}</span>
                <p className="item-text">{item.text}</p>
              </div>
              <svg viewBox="0 0 24 24" width="12" height="12" className="item-arrow">
                <path fill="currentColor" d="M8.59,16.58L13.17,12L8.59,7.41L10,6L16,12L10,18L8.59,16.58Z" />
              </svg>
            </a>
          ))}
          {latest.length === 0 && <p className="empty-msg">No entries yet</p>}
        </div>
      </section>
    </div>
  );
}

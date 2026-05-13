import { useState, useCallback } from 'react';

export function usePageContent() {
  const [content, setContent] = useState<string>('');
  const [loading, setLoading] = useState(false);

  const refreshContent = useCallback(async () => {
    setLoading(true);
    try {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (!tab?.id) return;

      const results = await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: () => {
          // Inline extraction logic since we can't easily import into the injected script context
          // without bundling it specifically.
          const doc = document.cloneNode(true) as Document;
          const selectorsToRemove = [
            'header', 'footer', 'nav', 'aside', 'script', 'style', 'iframe', 'noscript',
            '.header', '.footer', '.nav', '.navigation', '.sidebar', '.menu', '.ad', '.ads',
            '#header', '#footer', '#nav', '#navigation', '#sidebar', '#menu'
          ];
          selectorsToRemove.forEach(s => doc.querySelectorAll(s).forEach(el => el.remove()));
          let text = doc.body?.innerText || doc.body?.textContent || '';
          return text.replace(/\s+/g, ' ').trim();
        }
      });

      if (results?.[0]?.result) {
        setContent(results[0].result);
      }
    } catch (err) {
      console.error('Failed to extract page content:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  return { content, loading, refreshContent };
}

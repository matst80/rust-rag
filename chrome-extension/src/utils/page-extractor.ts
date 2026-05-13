/**
 * Extracts the main content from the current document by stripping
 * headers, footers, navs, scripts, and other irrelevant elements.
 */
export function extractMainContent(): string {
  const doc = document.cloneNode(true) as Document;
  
  // Elements to remove
  const selectorsToRemove = [
    'header', 'footer', 'nav', 'aside', 'script', 'style', 'iframe', 'noscript',
    '.header', '.footer', '.nav', '.navigation', '.sidebar', '.menu', '.ad', '.ads',
    '#header', '#footer', '#nav', '#navigation', '#sidebar', '#menu',
    '[role="banner"]', '[role="navigation"]', '[role="contentinfo"]', '.cookie-banner', '.social-share'
  ];

  selectorsToRemove.forEach(selector => {
    doc.querySelectorAll(selector).forEach(el => el.remove());
  });

  const body = doc.body;
  if (!body) return '';

  // Better text extraction: walk the tree and get text from visible elements
  // For now, innerText is actually quite good as it respects some CSS layout (like display:none)
  // but since we are on a cloned node without styles, we'll just use a simple approach.
  
  // Remove hidden elements (if they have inline styles)
  doc.querySelectorAll('[style*="display: none"], [style*="visibility: hidden"]').forEach(el => el.remove());

  let text = body.innerText || body.textContent || '';
  
  // Collapse whitespace and normalize
  text = text
    .replace(/\t/g, ' ')
    .replace(/ +/g, ' ')
    .replace(/\n\s*\n/g, '\n\n')
    .trim();
  
  return text;
}

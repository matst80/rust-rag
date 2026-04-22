// popup.js

document.addEventListener('DOMContentLoaded', async () => {
  const elements = {
    settingsBtn: document.getElementById('settings-btn'),
    settingsModal: document.getElementById('settings-modal'),
    saveSettings: document.getElementById('save-settings'),
    closeSettings: document.getElementById('close-settings'),
    apiBaseUrlInput: document.getElementById('api-base-url'),
    apiTokenInput: document.getElementById('api-token'),
    
    tabBtns: document.querySelectorAll('.tab-btn'),
    tabContents: document.querySelectorAll('.tab-content'),
    
    searchInput: document.getElementById('search-input'),
    searchBtn: document.getElementById('search-btn'),
    searchResults: document.getElementById('search-results'),
    
    storeInput: document.getElementById('store-input'),
    sourceSelect: document.getElementById('source-select'),
    storeBtn: document.getElementById('store-btn'),
    
    startAuthBtn: document.getElementById('start-auth-btn'),
    codeDisplay: document.getElementById('code-display'),
    userCodeSpan: document.getElementById('user-code'),
    verificationUrlA: document.getElementById('verification-url'),
    authSection: document.getElementById('auth-section'),
    mainSection: document.getElementById('main-section'),
    authStatus: document.getElementById('auth-status'),
    logoutBtn: document.getElementById('logout-btn'),
    
    connectionStatus: document.getElementById('connection-status'),
    apiUrlDisplay: document.getElementById('api-url-display')
  };

  let config = await chrome.storage.local.get(['apiBaseUrl', 'apiToken']);
  const defaultUrl = 'http://localhost:4001';
  
  if (!config.apiBaseUrl) {
    config.apiBaseUrl = defaultUrl;
    await chrome.storage.local.set({ apiBaseUrl: defaultUrl });
  }

  elements.apiBaseUrlInput.value = config.apiBaseUrl;
  elements.apiTokenInput.value = config.apiToken || '';
  elements.apiUrlDisplay.textContent = config.apiBaseUrl;

  // Initialize status
  checkConnection();

  // Tab switching
  elements.tabBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      const tabId = btn.getAttribute('data-tab');
      elements.tabBtns.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      
      elements.tabContents.forEach(content => {
        if (content.id === `${tabId}-tab`) {
          content.classList.remove('hidden');
        } else {
          content.classList.add('hidden');
        }
      });
    });
  });

  // Settings
  elements.settingsBtn.addEventListener('click', () => {
    elements.settingsModal.classList.remove('hidden');
    // If not authorized, show auth section in main
    if (!config.apiToken) {
        elements.authSection.classList.remove('hidden');
        elements.mainSection.classList.add('hidden');
    }
  });

  elements.closeSettings.addEventListener('click', () => {
    elements.settingsModal.classList.add('hidden');
  });

  elements.saveSettings.addEventListener('click', async () => {
    const url = elements.apiBaseUrlInput.value.trim().replace(/\/$/, "");
    const token = elements.apiTokenInput.value.trim();
    
    await chrome.storage.local.set({ apiBaseUrl: url, apiToken: token });
    config.apiBaseUrl = url;
    config.apiToken = token;
    
    elements.apiUrlDisplay.textContent = url;
    elements.settingsModal.classList.add('hidden');
    checkConnection();
  });

  // Search
  elements.searchBtn.addEventListener('click', performSearch);
  elements.searchInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') performSearch();
  });

  async function performSearch() {
    const query = elements.searchInput.value.trim();
    if (!query) return;

    elements.searchResults.innerHTML = '<div class="polling-status"><div class="spinner"></div><span>Searching...</span></div>';

    try {
      const response = await fetch(`${config.apiBaseUrl}/api/search`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': config.apiToken ? `Bearer ${config.apiToken}` : ''
        },
        body: JSON.stringify({ query, top_k: 5 })
      });

      if (!response.ok) throw new Error('Search failed');

      const data = await response.json();
      renderResults(data.results);
    } catch (error) {
      elements.searchResults.innerHTML = `<p class="placeholder" style="color: var(--error-color)">Error: ${error.message}</p>`;
    }
  }

  function renderResults(results) {
    if (!results || results.length === 0) {
      elements.searchResults.innerHTML = '<p class="placeholder">No results found.</p>';
      return;
    }

    elements.searchResults.innerHTML = results.map(res => `
      <div class="result-item">
        <div class="result-text">${escapeHtml(res.text)}</div>
        <div class="result-meta">
          <span>${res.source_id}</span>
          <span>${Math.round(res.score * 100)}% match</span>
        </div>
      </div>
    `).join('');
  }

  // Store
  elements.storeBtn.addEventListener('click', async () => {
    const text = elements.storeInput.value.trim();
    if (!text) return;

    elements.storeBtn.disabled = true;
    elements.storeBtn.textContent = 'Storing...';

    try {
      const response = await fetch(`${config.apiBaseUrl}/api/store`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': config.apiToken ? `Bearer ${config.apiToken}` : ''
        },
        body: JSON.stringify({
          text,
          source_id: elements.sourceSelect.value,
          metadata: {
            method: 'chrome-popup',
            stored_at: new Date().toISOString()
          }
        })
      });

      if (!response.ok) throw new Error('Store failed');

      elements.storeInput.value = '';
      elements.storeBtn.textContent = 'Success!';
      setTimeout(() => {
        elements.storeBtn.textContent = 'Store Entry';
        elements.storeBtn.disabled = false;
      }, 2000);
    } catch (error) {
      alert(`Error: ${error.message}`);
      elements.storeBtn.disabled = false;
      elements.storeBtn.textContent = 'Store Entry';
    }
  });

  // Device Auth Flow
  elements.startAuthBtn.addEventListener('click', async () => {
    try {
      const response = await fetch(`${config.apiBaseUrl}/auth/device/code`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ client_name: 'Chrome Extension' })
      });

      if (!response.ok) throw new Error('Failed to get device code');

      const data = await response.json();
      elements.userCodeSpan.textContent = data.user_code;
      elements.verificationUrlA.href = data.verification_uri_complete;
      elements.verificationUrlA.textContent = data.verification_uri;
      elements.codeDisplay.classList.remove('hidden');
      elements.startAuthBtn.classList.add('hidden');

      pollForToken(data.device_code, data.interval);
    } catch (error) {
      alert(`Auth Error: ${error.message}`);
    }
  });

  async function pollForToken(deviceCode, interval) {
    const pollInterval = (interval || 5) * 1000;
    
    const poll = async () => {
      try {
        const response = await fetch(`${config.apiBaseUrl}/auth/device/token`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ device_code: deviceCode })
        });

        const data = await response.json();

        if (response.ok && data.access_token) {
          await chrome.storage.local.set({ apiToken: data.access_token });
          config.apiToken = data.access_token;
          elements.apiTokenInput.value = data.access_token;
          showAuthorized();
          return;
        }

        if (data.error === 'authorization_pending') {
          setTimeout(poll, pollInterval);
        } else {
          throw new Error(data.error || 'Auth failed');
        }
      } catch (error) {
        if (error.message === 'slow_down') {
            setTimeout(poll, pollInterval + 2000);
        } else {
            alert(`Polling Error: ${error.message}`);
            elements.startAuthBtn.classList.remove('hidden');
            elements.codeDisplay.classList.add('hidden');
        }
      }
    };

    setTimeout(poll, pollInterval);
  }

  function showAuthorized() {
    elements.authStatus.classList.remove('hidden');
    elements.deviceCodeFlow.classList.add('hidden');
    elements.authSection.classList.remove('hidden');
    elements.mainSection.classList.remove('hidden');
    checkConnection();
  }

  elements.logoutBtn.addEventListener('click', async () => {
    await chrome.storage.local.remove('apiToken');
    config.apiToken = null;
    elements.apiTokenInput.value = '';
    elements.authStatus.classList.add('hidden');
    elements.deviceCodeFlow.classList.remove('hidden');
    elements.startAuthBtn.classList.remove('hidden');
    elements.codeDisplay.classList.add('hidden');
    checkConnection();
  });

  async function checkConnection() {
    try {
      const response = await fetch(`${config.apiBaseUrl}/api/graph/status`, {
        headers: {
          'Authorization': config.apiToken ? `Bearer ${config.apiToken}` : ''
        }
      });
      if (response.ok) {
        elements.connectionStatus.textContent = 'Online';
        elements.connectionStatus.className = 'status-indicator online';
        
        // If we have a token and connection is ok, make sure main section is visible
        if (config.apiToken) {
            elements.authSection.classList.add('hidden');
            elements.mainSection.classList.remove('hidden');
        } else {
            elements.authSection.classList.remove('hidden');
            elements.mainSection.classList.add('hidden');
        }
      } else {
        throw new Error();
      }
    } catch (e) {
      elements.connectionStatus.textContent = 'Offline';
      elements.connectionStatus.className = 'status-indicator offline';
    }
  }

  function escapeHtml(unsafe) {
    return unsafe
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }
});

// background.js

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "store-in-rag",
    title: "Store in RAG",
    contexts: ["selection"]
  });
  chrome.contextMenus.create({
    id: "smart-store-in-rag",
    title: "Smart save to RAG (AI-assisted)",
    contexts: ["selection"]
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "store-in-rag") {
    storeSelection(info.selectionText, tab.url, tab.title);
  } else if (info.menuItemId === "smart-store-in-rag") {
    smartStoreSelection(info.selectionText, tab.url, tab.title);
  }
});

chrome.commands.onCommand.addListener((command) => {
  if (command === "store-selection") {
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      chrome.scripting.executeScript({
        target: { tabId: tabs[0].id },
        func: () => window.getSelection().toString()
      }, (results) => {
        if (results && results[0].result) {
          storeSelection(results[0].result, tabs[0].url, tabs[0].title);
        }
      });
    });
  } else if (command === "smart-store-selection") {
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      chrome.scripting.executeScript({
        target: { tabId: tabs[0].id },
        func: () => window.getSelection().toString()
      }, (results) => {
        if (results && results[0].result) {
          smartStoreSelection(results[0].result, tabs[0].url, tabs[0].title);
        }
      });
    });
  }
});

async function storeSelection(text, url, title) {
  const { apiBaseUrl, apiToken } = await chrome.storage.local.get(['apiBaseUrl', 'apiToken']);
  
  if (!apiBaseUrl || !apiToken) {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/icon48.png",
      title: "rust-rag",
      message: "Please configure API URL and Token in the extension popup."
    });
    return;
  }

  try {
    const response = await fetch(`${apiBaseUrl}/api/store`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Authorization": `Bearer ${apiToken}`
      },
      body: JSON.stringify({
        text: text,
        source_id: "chrome-extension",
        metadata: {
          url: url,
          title: title,
          stored_at: new Date().toISOString()
        }
      })
    });

    if (response.ok) {
      chrome.notifications.create({
        type: "basic",
        iconUrl: "icons/icon48.png",
        title: "rust-rag",
        message: "Successfully stored in RAG!"
      });
    } else {
      const error = await response.json();
      throw new Error(error.error || response.statusText);
    }
  } catch (error) {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/icon48.png",
      title: "rust-rag Error",
      message: `Failed to store: ${error.message}`
    });
  }
}

async function smartStoreSelection(text, url, title) {
  const { apiBaseUrl, apiToken } = await chrome.storage.local.get(['apiBaseUrl', 'apiToken']);

  if (!apiBaseUrl || !apiToken) {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/icon48.png",
      title: "rust-rag",
      message: "Please configure API URL and Token in the extension popup."
    });
    return;
  }

  try {
    const context = {};
    if (url) context.url = url;
    if (title) context.title = title;

    const response = await fetch(`${apiBaseUrl}/api/store/smart`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Authorization": `Bearer ${apiToken}`
      },
      body: JSON.stringify({
        text: text,
        context: Object.keys(context).length > 0 ? context : undefined
      })
    });

    if (response.ok) {
      const data = await response.json();
      const count = data.items?.length ?? 0;
      const sources = [...new Set((data.items ?? []).map(i => i.source_id))].join(', ');
      chrome.notifications.create({
        type: "basic",
        iconUrl: "icons/icon48.png",
        title: "rust-rag",
        message: `Smart saved ${count} item${count !== 1 ? 's' : ''} → ${sources}`
      });
    } else {
      const error = await response.json();
      throw new Error(error.error || response.statusText);
    }
  } catch (error) {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/icon48.png",
      title: "rust-rag Error",
      message: `Smart save failed: ${error.message}`
    });
  }
}

// Device Auth Polling in Background
let pollingState = {
  isPolling: false,
  deviceCode: null,
  userCode: null,
  verificationUri: null,
  verificationUriComplete: null,
  interval: 5,
  apiBaseUrl: null
};

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'START_POLLING') {
    startBackgroundPolling(message);
    sendResponse({ status: 'started' });
  } else if (message.type === 'GET_POLLING_STATUS') {
    sendResponse(pollingState);
  }
});

async function startBackgroundPolling(data) {
  if (pollingState.isPolling) return;
  
  pollingState = {
    isPolling: true,
    deviceCode: data.deviceCode,
    userCode: data.userCode,
    verificationUri: data.verificationUri,
    verificationUriComplete: data.verificationUriComplete,
    interval: data.interval || 5,
    apiBaseUrl: data.apiBaseUrl
  };

  const pollInterval = (pollingState.interval) * 1000;
  const apiBaseUrl = pollingState.apiBaseUrl;
  const deviceCode = pollingState.deviceCode;

  const poll = async () => {
    if (!pollingState.isPolling || pollingState.deviceCode !== deviceCode) return;

    try {
      const response = await fetch(`${apiBaseUrl}/auth/device/token`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ device_code: deviceCode })
      });

      const resData = await response.json();

      if (response.ok && resData.access_token) {
        await chrome.storage.local.set({ apiToken: resData.access_token });
        pollingState.isPolling = false;
        
        chrome.notifications.create({
          type: "basic",
          iconUrl: "icons/icon48.png",
          title: "rust-rag",
          message: "Device successfully authorized!"
        });
        
        // Notify popup if it's open
        chrome.runtime.sendMessage({ type: 'AUTH_SUCCESS', token: resData.access_token });
        return;
      }

      if (resData.error === 'authorization_pending') {
        setTimeout(poll, pollInterval);
      } else {
        throw new Error(resData.error || 'Auth failed');
      }
    } catch (error) {
      if (error.message === 'slow_down') {
        setTimeout(poll, pollInterval + 2000);
      } else {
        pollingState.isPolling = false;
        chrome.runtime.sendMessage({ type: 'AUTH_ERROR', error: error.message });
      }
    }
  };

  setTimeout(poll, pollInterval);
}

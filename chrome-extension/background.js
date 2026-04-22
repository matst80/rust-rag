// background.js

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "store-in-rag",
    title: "Store in RAG",
    contexts: ["selection"]
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "store-in-rag") {
    storeSelection(info.selectionText, tab.url, tab.title);
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

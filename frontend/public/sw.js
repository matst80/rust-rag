self.addEventListener('install', (event) => {
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(clients.claim());
});

self.addEventListener('fetch', () => {
  // Pass-through for now, just to satisfy PWA requirements. Deliberately no
  // respondWith(): re-dispatching `fetch(event.request)` from the worker turns
  // any network hiccup into a rejected promise and a hard network-error page;
  // the browser's default handling recovers on retry instead.
});

// Web Push handler. Payload shape (set by crate::notify):
//   { title, body, url?, tag?, data? }
self.addEventListener('push', (event) => {
  let payload = {};
  try {
    payload = event.data ? event.data.json() : {};
  } catch (_) {
    payload = { title: 'rust-rag', body: event.data ? event.data.text() : '' };
  }
  const title = payload.title || 'rust-rag';
  const options = {
    body: payload.body || '',
    tag: payload.tag,
    data: { url: payload.url, ...(payload.data || {}) },
    icon: '/icon-192.png',
    badge: '/icon-192.png',
  };
  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const target = (event.notification.data && event.notification.data.url) || '/';
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clientList) => {
      for (const client of clientList) {
        if ('focus' in client && client.url.endsWith(target)) {
          return client.focus();
        }
      }
      if (self.clients.openWindow) return self.clients.openWindow(target);
      return null;
    }),
  );
});

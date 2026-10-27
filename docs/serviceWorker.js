// Kill switch for a stale service worker (bd-73lilznv). Until 2026-10 this
// URL served the hub-client sandboxed preview, whose service worker took
// over every request under /q2/ and answers page loads with a 502 when it is
// not inside quarto-hub.com. Browsers that still have it registered fetch
// this file on their next visit, replace the old worker with this one, and
// this one unregisters itself and reloads the open pages, which then come
// straight from GitHub Pages. Safe to delete no earlier than about 2027-04.
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil((async () => {
  await self.registration.unregister();
  for (const c of await self.clients.matchAll({ type: 'window' })) c.navigate(c.url);
})()));

/* Signal Generator PWA — service worker
 * Стратегия:
 *  - статика (app.js, иконки, манифест): cache-first с фоновым обновлением (stale-while-revalidate)
 *  - корень "/" (SSR, требует свежести): network-first, фолбэк на кэш при офлайне
 *  - API (/api/*): никогда не кэшируем — только сеть
 */
const CACHE = 'siggen-v1';
const SHELL = [
  '/',
  '/app.js',
  '/manifest.webmanifest',
  '/icons/icon_192.png',
  '/icons/icon_512.png',
  '/icons/icon_180.png',
];

self.addEventListener('install', (e) => {
  e.waitUntil(
    caches.open(CACHE)
      .then((c) => c.addAll(SHELL))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);

  // API и WebSocket — не трогаем
  if (url.pathname.startsWith('/api/') || url.pathname === '/ws') return;
  // Только GET same-origin
  if (e.request.method !== 'GET' || url.origin !== self.location.origin) return;

  // Навигация (корень) — network-first
  if (e.request.mode === 'navigate' || url.pathname === '/') {
    e.respondWith(
      fetch(e.request)
        .then((res) => {
          const copy = res.clone();
          caches.open(CACHE).then((c) => c.put('/', copy));
          return res;
        })
        .catch(() => caches.match('/').then((hit) => hit || caches.match('/').then((c) => c)))
    );
    return;
  }

  // Статика — stale-while-revalidate
  e.respondWith(
    caches.match(e.request).then((cached) => {
      const network = fetch(e.request)
        .then((res) => {
          if (res && res.ok) {
            const copy = res.clone();
            caches.open(CACHE).then((c) => c.put(e.request, copy));
          }
          return res;
        })
        .catch(() => cached);
      return cached || network;
    })
  );
});

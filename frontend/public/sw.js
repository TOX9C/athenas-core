const CACHE = 'athena-companion-v3';
const SHELL = [
  './mobile.html',
  './manifest.webmanifest',
  './styles.css',
  './mobile.css',
  './icons/athena.svg'
];

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(SHELL)));
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
  );
  self.clients.claim();
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.pathname.endsWith('/__athena_discovery__.json') || url.pathname.endsWith('/ws')) return;

  // Only fall back to the cached document for navigations. Never return
  // mobile.html for a failed JS/WASM/CSS request, which creates misleading
  // MIME errors and leaves the app on its boot screen.
  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request).catch(() => caches.match(request).then((cached) => cached || caches.match('./mobile.html')))
    );
  }
});

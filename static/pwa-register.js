/* PWA: регистрация service worker (только secure context) */
if ('serviceWorker' in navigator && (location.protocol === 'https:' || location.hostname === 'localhost' || location.hostname === '127.0.0.1')) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').then((reg) => {
      console.log('SW registered', reg.scope);
    }).catch((e) => console.warn('SW registration failed', e));
  });
}

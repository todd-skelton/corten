// App code for the generated service worker, loaded first by it (corten::pwa::Pwa::import_script).
// A push message is JSON: {"title", "body", "url"}. Sending one (VAPID keys, payload encryption,
// stored subscriptions) is the server's job; this file only shows it and opens the page on click.
const worker = /** @type {any} */ (self);

worker.addEventListener("push", (/** @type {any} */ event) => {
  const data = event.data ? event.data.json() : {};
  event.waitUntil(worker.registration.showNotification(data.title, { body: data.body, data: { url: data.url ?? "/" } }));
});

worker.addEventListener("notificationclick", (/** @type {any} */ event) => {
  event.notification.close();
  event.waitUntil(worker.clients.openWindow(event.notification.data.url));
});

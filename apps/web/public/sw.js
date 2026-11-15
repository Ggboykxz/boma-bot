// The Boma Bot push service worker (REQ-064).
//
// `push` arrives with a payload this deployment built: five keys, never conversation content.
// It shows a notification that names a bot and points at the thread; clicking it focuses an
// already-open thread where possible, else opens one.

self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data ? event.data.json() : {};
  } catch {
    data = {};
  }
  const title = typeof data.title === "string" && data.title ? data.title : "Boma Bot";
  const tag = typeof data.tag === "string" && data.tag ? data.tag : "boma-notification";
  const url = typeof data.url === "string" && data.url ? data.url : "/app/";
  event.waitUntil(
    self.registration.showNotification(title, {
      tag,
      data: { url },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  const notification = event.notification;
  notification.close();
  const url = notification?.data?.url ?? "/app/";
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({
        type: "window",
        includeUncontrolled: true,
      });
      for (const client of windows) {
        try {
          if ("navigate" in client && typeof client.navigate === "function") {
            await client.navigate(url);
          }
          return client.focus();
        } catch {
          // Fall through to the next window, or open a fresh one.
        }
      }
      return self.clients.openWindow(url);
    })(),
  );
});

self.addEventListener("install", (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

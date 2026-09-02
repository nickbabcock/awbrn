/**
 * The worker that draws a notification when no tab is open to hear one.
 *
 * It is deliberately thin: the wording and the destination are settled on the
 * server and arrive in the payload, because this file has no build step and
 * anything written here would be a second copy of them.
 */

self.addEventListener("install", () => {
  // Take over straight away, so a player who has just allowed notifications
  // does not have to close every tab before the first one can arrive.
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("push", (event) => {
  let payload = null;
  try {
    payload = event.data ? event.data.json() : null;
  } catch {
    payload = null;
  }
  if (!payload || payload.type !== "turnDigest") {
    return;
  }

  event.waitUntil(
    self.registration.showNotification(payload.title, {
      body: payload.body,
      // One tag for every turn notification, so a later one replaces the
      // notification still on screen rather than stacking beneath it.
      tag: "awbrn-turn",
      renotify: true,
      data: { url: payload.url },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = (event.notification.data && event.notification.data.url) || "/my/matches";

  event.waitUntil(
    (async () => {
      const clients = await self.clients.matchAll({
        type: "window",
        includeUncontrolled: true,
      });
      // A player who already has the site open should be brought to the tab
      // they have, not given a second one.
      for (const client of clients) {
        if (new URL(client.url).origin === self.location.origin) {
          await client.focus();
          if ("navigate" in client) {
            await client.navigate(url);
          }
          return;
        }
      }
      await self.clients.openWindow(url);
    })(),
  );
});

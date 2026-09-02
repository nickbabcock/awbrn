import { base64UrlDecode } from "./web_push.ts";
import { subscribePushFn, unsubscribePushFn } from "./players.functions.ts";

/** Where the service worker lives. */
const SERVICE_WORKER_URL = "/sw.js";

/** Everything it answers for, which has to be the whole site. */
const SERVICE_WORKER_SCOPE = "/";

export type PushPermission = "unsupported" | "default" | "granted" | "denied";

/**
 * Whether this browser can carry notifications at all.
 *
 * Several cannot, and one that is not on a secure origin cannot either, so
 * every entry point checks rather than letting a page offer something that
 * would fail when the player reached for it.
 */
export function isPushSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

export function pushPermission(): PushPermission {
  if (!isPushSupported()) return "unsupported";
  return Notification.permission as PushPermission;
}

async function registration(): Promise<ServiceWorkerRegistration> {
  return navigator.serviceWorker.register(SERVICE_WORKER_URL, { scope: SERVICE_WORKER_SCOPE });
}

/**
 * The subscription this browser already holds, or null.
 *
 * The registration is looked up by the scope it answers for rather than by the
 * script, because that is what `getRegistration` takes, and it does not
 * register one: a page only asking whether notifications are on should not
 * install a service worker to find out.
 */
export async function currentSubscription(): Promise<PushSubscription | null> {
  if (!isPushSupported()) return null;
  const existing = await navigator.serviceWorker.getRegistration(SERVICE_WORKER_SCOPE);
  return (await existing?.pushManager.getSubscription()) ?? null;
}

function subscriptionKey(subscription: PushSubscription, name: "p256dh" | "auth"): string {
  const key = subscription.getKey(name);
  if (key === null) {
    throw new Error(`the browser gave a subscription with no ${name} key`);
  }
  let binary = "";
  for (const byte of new Uint8Array(key)) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/**
 * Ask for permission, subscribe, and tell the site where to send.
 *
 * Returns false when the player refuses, which is an answer and not a failure,
 * so the caller reports the state rather than an error.
 */
export async function enablePush(vapidPublicKey: string): Promise<boolean> {
  if (!isPushSupported()) return false;

  const permission = await Notification.requestPermission();
  if (permission !== "granted") return false;

  const worker = await registration();
  // A browser already subscribed under a different key would be handed a
  // subscription the site cannot sign for, so the old one goes first.
  const existing = await worker.pushManager.getSubscription();
  if (existing) {
    await existing.unsubscribe();
  }

  const subscription = await worker.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: base64UrlDecode(vapidPublicKey) as BufferSource,
  });

  await subscribePushFn({
    data: {
      endpoint: subscription.endpoint,
      p256dh: subscriptionKey(subscription, "p256dh"),
      auth: subscriptionKey(subscription, "auth"),
      label: navigator.userAgent.slice(0, 80),
    },
  });
  return true;
}

/**
 * Stop this browser hearing anything, on the site and then on the browser.
 *
 * The site is told first. If that fails the browser keeps its subscription, so
 * the control still reads as on and pressing it again finishes the job. The
 * other order leaves a player who was told notifications are off still being
 * sent them, with nothing left on the browser to turn off again.
 */
export async function disablePush(): Promise<void> {
  const subscription = await currentSubscription();
  if (!subscription) return;
  await unsubscribePushFn({ data: { endpoint: subscription.endpoint } });
  await subscription.unsubscribe();
}

import { createServerFn } from "@tanstack/react-start";
import { env } from "cloudflare:workers";
import { sessionMiddleware } from "#/auth/session.middleware.ts";
import { requireRateLimit } from "#/rate_limit.ts";
import { getPlayerStub } from "./player_service.ts";
import { pushSubscriptionSchema, pushUnsubscribeSchema } from "./schemas.ts";
import { readVapidKeys, type VapidEnvironment } from "./web_push.ts";

/**
 * What a browser needs before it can subscribe.
 *
 * The key is the public half of the pair notifications are signed with, so it
 * is meant to be handed out. A deployment without one reports null and the
 * page offers nothing rather than failing.
 */
export const pushConfigFn = createServerFn({ method: "GET" }).handler(() => {
  return { publicKey: readVapidKeys(env as VapidEnvironment)?.publicKey ?? null };
});

export const subscribePushFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(pushSubscriptionSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    try {
      await requireRateLimit(env.PUSH_SUBSCRIBE_RATE_LIMITER, `user:${context.session.user.id}`);
    } catch (response) {
      if (response instanceof Response) throw response;
      throw response;
    }
    await getPlayerStub(context.session.user.id).addPushSubscription(data);
    return { subscribed: true };
  });

export const unsubscribePushFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(pushUnsubscribeSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    await getPlayerStub(context.session.user.id).removePushSubscription(data.endpoint);
    return { subscribed: false };
  });

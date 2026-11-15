import { createServerFn } from "@tanstack/react-start";
import { env } from "cloudflare:workers";
import { sessionMiddleware } from "#/auth/session.middleware.ts";
import { requireFnRateLimit } from "#/rate_limit.ts";
import { listCompletedMatches } from "#/matches/matches.server.ts";
import { getPlayerStub } from "./player_service.ts";
import { readPlayerProfile } from "./players.server.ts";
import {
  playerHistoryRequestSchema,
  playerProfileRequestSchema,
  pushSubscriptionSchema,
  pushUnsubscribeSchema,
} from "./schemas.ts";
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
    await requireFnRateLimit(env.PUSH_SUBSCRIBE_RATE_LIMITER, `user:${context.session.user.id}`);
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

/** A player's profile. Public: a visitor who is not signed in reads it too. */
export const getPlayerProfileFn = createServerFn({ method: "GET" })
  .middleware([sessionMiddleware])
  .validator(playerProfileRequestSchema)
  .handler(({ data, context }) => readPlayerProfile(data.userId, context.session?.user.id ?? null));

/** A player's finished matches, a page at a time; invite-only ones only to the player. */
export const listPlayerCompletedMatchesFn = createServerFn({ method: "GET" })
  .middleware([sessionMiddleware])
  .validator(playerHistoryRequestSchema)
  .handler(async ({ data, context }) => {
    const result = await listCompletedMatches(
      data.userId,
      { cursor: data.cursor },
      { includePrivate: context.session?.user.id === data.userId },
    );
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

import { createServerFn } from "@tanstack/react-start";
import { z } from "zod";
import { sessionMiddleware } from "#/auth/session.middleware.ts";
import {
  actorMiddleware,
  optionalActorMiddleware,
  requirePermission,
} from "#/auth/permission.middleware.ts";
import { getFactionById } from "#/factions.ts";
import { matchIdSchema } from "./match_id.ts";
import {
  createMatch,
  getMatchSnapshot,
  listMatches,
  listMyCompletedMatches,
  listMyMatches,
  mutateMatch,
  voidMatch,
} from "./matches.server";
import {
  matchBrowseRequestSchema,
  matchCreateRequestSchema,
  matchHistoryRequestSchema,
  matchMutationRequestSchema,
  matchVoidRequestSchema,
} from "./schemas";
import { rateLimitBindings, requireRateLimit } from "#/rate_limit.ts";

export const listMatchesFn = createServerFn({ method: "GET" })
  .validator(matchBrowseRequestSchema)
  .handler(async ({ data }) => {
    const result = await listMatches(data);
    if (!result.ok) throw new Error(result.error.message);
    return { ...result.value, loadedAt: new Date().toISOString() };
  });

export const listMyMatchesFn = createServerFn({ method: "GET" })
  .middleware([sessionMiddleware])
  .handler(async ({ context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    const result = await listMyMatches(context.session.user.id);
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

export const listMyCompletedMatchesFn = createServerFn({ method: "GET" })
  .middleware([sessionMiddleware])
  .validator(matchHistoryRequestSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    const result = await listMyCompletedMatches(context.session.user.id, data);
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

/**
 * Both middlewares run because they answer different questions. The session
 * says who the viewer is, which is what decides whether they hold a seat.
 * The actor says what they may do, and is null for a banned account: a ban
 * takes away what a role granted without taking away who they are.
 */
export const getMatchFn = createServerFn({ method: "GET" })
  .middleware([sessionMiddleware, optionalActorMiddleware])
  .validator(z.object({ matchId: matchIdSchema, joinSlug: z.string().nullish() }))
  .handler(async ({ data, context }) => {
    const result = await getMatchSnapshot(
      data.matchId,
      context.session?.user.id ?? null,
      data.joinSlug ?? null,
      context.actor,
    );
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

export const createMatchFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware, actorMiddleware])
  .validator(matchCreateRequestSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Error("you must be signed in to create a match");
    await requireRateLimit(
      rateLimitBindings().CREATE_MATCH_RATE_LIMITER,
      `user:${context.session.user.id}`,
    );
    const result = await createMatch(data, {
      id: context.actor.userId,
      name: context.session.user.name,
    });
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

export const mutateMatchFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(z.object({ matchId: matchIdSchema, action: matchMutationRequestSchema }))
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Error("you must be signed in to update a lobby");
    await Promise.all([
      requireRateLimit(
        rateLimitBindings().LOBBY_WRITE_RATE_LIMITER,
        `user:${context.session.user.id}`,
        10,
      ),
      requireRateLimit(
        rateLimitBindings().LOBBY_WRITE_RATE_LIMITER,
        `match:${data.matchId}:user:${context.session.user.id}`,
        10,
      ),
    ]);

    const { action } = data;
    if (action.action === "updateParticipant") {
      if (
        action.factionId === undefined &&
        action.coId === undefined &&
        action.ready === undefined
      ) {
        throw new Error("no participant changes were provided");
      }
    }
    if (
      (action.action === "join" || action.action === "updateParticipant") &&
      action.factionId !== undefined &&
      getFactionById(action.factionId) === null
    ) {
      throw new Error("factionId must reference a valid faction");
    }

    const result = await mutateMatch(data.matchId, context.session.user, action);
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

/**
 * Mark a match as not counting.
 *
 * There is no owner branch here on purpose: a player who could void the
 * match they lost would hold a way out of every loss, so this is a moderator
 * act and nothing else.
 */
export const voidMatchFn = createServerFn({ method: "POST" })
  .middleware([requirePermission({ match: ["void"] })])
  .validator(matchVoidRequestSchema)
  .handler(async ({ data, context }) => {
    const result = await voidMatch({
      matchId: data.matchId,
      publicReason: data.publicReason,
      reason: data.reason,
      actor: context.actor,
    });
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

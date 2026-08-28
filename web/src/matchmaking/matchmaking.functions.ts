import { createServerFn } from "@tanstack/react-start";
import { z } from "zod";
import { sessionMiddleware } from "#/auth/session.middleware.ts";
import { matchIdSchema } from "#/matches/match_id.ts";
import { rankedConfirmationRequestSchema, rankedPoolSchema } from "#/matches/schemas.ts";
import { rateLimitBindings, requireRateLimit } from "#/rate_limit.ts";
import { DEFAULT_MAX_ACTIVE_MATCHES, HARD_MAX_ACTIVE_MATCHES } from "./matchmaking.ts";
import { listSeeks, startSeek, stopSeek, updateRankedConfirmation } from "./matchmaking.server.ts";

const seekRequestSchema = z.object({
  pool: rankedPoolSchema,
  maxActiveMatches: z
    .number()
    .int()
    .min(1)
    .max(HARD_MAX_ACTIVE_MATCHES)
    .default(DEFAULT_MAX_ACTIVE_MATCHES),
});

export const listRankedSeeksFn = createServerFn({ method: "GET" })
  .middleware([sessionMiddleware])
  .handler(async ({ context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    return listSeeks(context.session.user.id);
  });

export const startRankedSeekFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(seekRequestSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    await requireRateLimit(
      rateLimitBindings().LOBBY_WRITE_RATE_LIMITER,
      `seek:user:${context.session.user.id}`,
      10,
    );
    return startSeek(context.session.user.id, data.pool, data.maxActiveMatches);
  });

export const stopRankedSeekFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(z.object({ pool: rankedPoolSchema }))
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    await stopSeek(context.session.user.id, data.pool);
    return { stopped: true };
  });

export const updateRankedConfirmationFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(z.object({ matchId: matchIdSchema, action: rankedConfirmationRequestSchema }))
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Response("Unauthorized", { status: 401 });
    await updateRankedConfirmation(data.matchId, context.session.user.id, data.action);
    return { updated: true };
  });

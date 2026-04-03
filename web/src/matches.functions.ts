import { createServerFn } from "@tanstack/react-start";
import { z } from "zod";
import { getFactionById } from "./factions";
import { sessionMiddleware } from "./middleware";
import { createMatch, getMatchSnapshot, mutateMatch } from "./matches.server";
import { matchCreateRequestSchema, matchMutationRequestSchema } from "./schemas";

export const getMatchFn = createServerFn({ method: "GET" })
  .middleware([sessionMiddleware])
  .inputValidator(z.object({ matchId: z.string(), joinSlug: z.string().nullish() }))
  .handler(async ({ data, context }) => {
    const result = await getMatchSnapshot(
      data.matchId,
      context.session?.user.id ?? null,
      data.joinSlug ?? null,
    );
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

export const createMatchFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .inputValidator(matchCreateRequestSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Error("you must be signed in to create a match");
    const result = await createMatch(data, {
      id: context.session.user.id,
      name: context.session.user.name,
    });
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  });

export const mutateMatchFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .inputValidator(z.object({ matchId: z.string(), action: matchMutationRequestSchema }))
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Error("you must be signed in to update a lobby");

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

import { createMiddleware } from "@tanstack/react-start";
import { getRequest } from "@tanstack/react-start/server";
import type { Permissions } from "./access.ts";
import type { Actor } from "./actor.ts";
import { requireActor, resolveActor } from "./actor.server.ts";

/**
 * Puts the actor a write is checked against in the handler context.
 *
 * Use this when the answer needs the row: "the author of this map, or a
 * moderator" cannot be decided before the map is loaded. Use
 * `requirePermission` when the answer needs only the role.
 */
export const actorMiddleware = createMiddleware().server(async ({ next }) => {
  const actor: Actor = await requireActor(getRequest());
  return next({ context: { actor } });
});

/**
 * Puts the actor in the context, or null when nobody is signed in.
 *
 * For a read that anonymous visitors also get, where a role only widens what
 * comes back: a moderator seeing a private match, say.
 */
export const optionalActorMiddleware = createMiddleware().server(async ({ next }) => {
  const actor: Actor | null = await resolveActor(getRequest());
  return next({ context: { actor } });
});

/**
 * Closes a server function to anyone whose role does not hold these actions.
 *
 * The handler still gets the actor, because an act that passes this gate
 * usually has to be written to the moderation log with a name on it.
 */
export function requirePermission(permissions: Permissions) {
  return createMiddleware()
    .middleware([actorMiddleware])
    .server(async ({ next, context }) => {
      if (!context.actor.can(permissions)) {
        throw new Response("Forbidden", { status: 403 });
      }
      return next();
    });
}

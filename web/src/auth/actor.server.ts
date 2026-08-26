import { env } from "cloudflare:workers";
import { eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { user } from "#/db/global.ts";
import { actorFromRole, type Actor } from "./actor.ts";
import { getRequestSession } from "./auth.server.ts";
import { isBanned } from "./ban.ts";

/**
 * The actor a write is checked against.
 *
 * The role is read from the database rather than from the session, because
 * the session cookie is cached for minutes and a role that was taken away
 * must stop working at once. A banned user resolves to no actor at all, so a
 * ban closes every write in one place instead of one check for each.
 */
export async function requireActor(request: Request): Promise<Actor> {
  const actor = await resolveActor(request);
  if (!actor) throw new Response("Unauthorized", { status: 401 });
  return actor;
}

/**
 * The same actor, for a read that anonymous visitors are also allowed.
 *
 * A banned user resolves to null rather than to an error, so a ban takes
 * away what a role granted and leaves what everyone can see.
 */
export async function resolveActor(request: Request): Promise<Actor | null> {
  const session = await getRequestSession(request);
  if (!session) return null;

  const db = drizzle(env.DB, { schema: { user } });
  const row = await db
    .select({ role: user.role, banned: user.banned, banExpires: user.banExpires })
    .from(user)
    .where(eq(user.id, session.user.id))
    .get();

  if (!row) return null;
  if (isBanned(row.banned, row.banExpires)) return null;

  return actorFromRole(session.user.id, row.role);
}

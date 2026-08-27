import { DEFAULT_ROLE, roleAllows, type Permissions } from "./access.ts";
import type { Session } from "./session.ts";

/**
 * Who is acting, and what they are allowed to do.
 *
 * `can` reads the role that was resolved when the actor was built, so it is
 * pure. That is what lets an ownership rule such as `mapTagGrant` stay a
 * plain function that a test calls without a database and a screen calls
 * without a round trip.
 */
export interface Actor {
  userId: string;
  role: string;
  can(permissions: Permissions): boolean;
}

export function actorFromRole(userId: string, role: string | null | undefined): Actor {
  const resolved = role ?? DEFAULT_ROLE;
  return {
    userId,
    role: resolved,
    can: (permissions) => roleAllows(resolved, permissions),
  };
}

/**
 * The actor a screen draws with, built from the session it already holds.
 *
 * The role here comes from the session cookie, which is cached, so it lags a
 * change of role by as much as the cache holds. That is correct for deciding
 * which buttons to draw and wrong for deciding whether a write goes through:
 * a write resolves its actor with `requireActor`, which reads the database.
 */
export function viewerActor(session: Session | null | undefined): Actor | null {
  if (!session) return null;
  return actorFromRole(session.user.id, session.user.role);
}

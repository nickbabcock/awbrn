import type { Actor } from "#/auth/actor.ts";

/**
 * Why an act on a map is allowed, or null when it is not.
 *
 * The reason is carried rather than a plain yes, because a moderator editing
 * somebody else's map should not be shown the same button as its author. A
 * screen that draws "Retag" for an author draws "Retag as moderator" for a
 * moderator, and asks that moderator for a reason.
 */
export type MapGrant = "owner" | "moderator" | null;

export interface MapOwnership {
  authorUserId: string | null;
}

/**
 * Who may change the tags on a map: its author, or a moderator.
 *
 * Ownership is read first. An author who is also a moderator gets `owner`,
 * so the map they wrote does not ask them for a moderation reason.
 */
export function mapTagGrant(map: MapOwnership, actor: Actor | null): MapGrant {
  if (actor === null) return null;
  if (map.authorUserId !== null && map.authorUserId === actor.userId) {
    return actor.can({ map: ["tag"] }) ? "owner" : null;
  }
  return actor.can({ map: ["tag", "edit-any"] }) ? "moderator" : null;
}

/**
 * Who may rank a map revision: a moderator, and never its author.
 *
 * A rank is this site's judgement of a map and feeds the board a player
 * picks from, so an author ranking their own work is the one case the rule
 * exists to stop. Ownership is read before the role, and the role cannot
 * reach past it: a moderator who wrote the map is refused the same as
 * anybody else who wrote it. There is no `owner` grant here, because there
 * is no act an owner may do.
 */
export function mapRankGrant(map: MapOwnership, actor: Actor | null): MapGrant {
  if (actor === null) return null;
  if (map.authorUserId !== null && map.authorUserId === actor.userId) return null;
  return actor.can({ map: ["rank"] }) ? "moderator" : null;
}

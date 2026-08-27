import type { Actor } from "#/auth/actor.ts";

/**
 * Why an act on a match is allowed, or null when it is not.
 *
 * Neither act below has an ownership branch. Voiding is the judgement that a
 * match did not count, and a player who could void the match they lost would
 * hold a way out of every loss. Seeing a private match is the same rule read
 * the other way: taking part is already checked, and this is what reaches
 * past it.
 */
export type MatchGrant = "moderator" | null;

export function matchVoidGrant(actor: Actor | null): MatchGrant {
  if (actor === null) return null;
  return actor.can({ match: ["void"] }) ? "moderator" : null;
}

export function matchViewAnyGrant(actor: Actor | null): MatchGrant {
  if (actor === null) return null;
  return actor.can({ match: ["view-any"] }) ? "moderator" : null;
}

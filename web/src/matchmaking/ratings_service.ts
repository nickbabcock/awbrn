import type { RankedPool } from "#/matches/schemas.ts";

export type RatingsStub = DurableObjectStub<
  import("./ratings_durable_object.ts").RatingsDurableObject
>;

/**
 * The object which owns one pool's ratings.
 *
 * The name carries the pool and not the season. A rating belongs to a player
 * and a pool and it carries across seasons, so its writer is named the way the
 * row is keyed. A name with the season in it would give one rating row two
 * writers whenever an async match of the old season finished after the new one
 * opened, which is every season boundary.
 */
export function ratingsDurableObjectName(pool: RankedPool): string {
  return `ratings:${pool}`;
}

export function getRatingsStub(
  binding: DurableObjectNamespace<import("./ratings_durable_object.ts").RatingsDurableObject>,
  pool: RankedPool,
): RatingsStub {
  return binding.getByName(ratingsDurableObjectName(pool));
}

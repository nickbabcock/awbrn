import type { RankedPool } from "#/matches/schemas.ts";

export type MatchmakerStub = DurableObjectStub<
  import("./matchmaker_durable_object.ts").MatchmakerDurableObject
>;

export function matchmakerDurableObjectName(season: number, pool: RankedPool): string {
  return `matchmaker:${season}:${pool}`;
}

export function getMatchmakerStub(
  binding: DurableObjectNamespace<import("./matchmaker_durable_object.ts").MatchmakerDurableObject>,
  season: number,
  pool: RankedPool,
): MatchmakerStub {
  return binding.getByName(matchmakerDurableObjectName(season, pool));
}

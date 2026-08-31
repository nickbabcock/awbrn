import { queryOptions } from "@tanstack/react-query";
import type { RankedPool } from "#/matches/schemas.ts";
import { rankedOverviewFn, rankedStandingsFn } from "./matchmaking.functions.ts";
import { rankedKeys } from "./matchmaking.keys.ts";

/**
 * How often the hub asks again while a seek is running.
 *
 * A pairing arrives from the server without a channel to announce it, so the
 * hub polls. The interval is slow, because an async pairing is not an event
 * the player waits at the screen for.
 */
export const RANKED_POLL_INTERVAL_MS = 30_000;

export function rankedOverviewQueryOptions() {
  return queryOptions({
    queryKey: rankedKeys.overview(),
    queryFn: () => rankedOverviewFn(),
  });
}

export function rankedStandingsQueryOptions(pool: RankedPool) {
  return queryOptions({
    queryKey: rankedKeys.standings(pool),
    queryFn: () => rankedStandingsFn({ data: { pool } }),
  });
}

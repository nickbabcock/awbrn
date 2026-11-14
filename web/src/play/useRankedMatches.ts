import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { clockTickMs } from "#/matches/match_clock.ts";
import { rankedOverviewQueryOptions } from "#/matchmaking/matchmaking.queries.ts";

export interface RankedMatchFacts {
  /** The window each pending pairing has to be confirmed in, by match. */
  confirmDeadlines: ReadonlyMap<string, number>;
  /**
   * Every match the ranked pools hold for the viewer, pending or in play, with
   * the name of the map it is titled by.
   */
  rankedMaps: ReadonlyMap<string, string>;
}

const NO_FACTS: RankedMatchFacts = { confirmDeadlines: new Map(), rankedMaps: new Map() };

/**
 * What the ranked overview knows about the viewer's matches that the list of
 * matches does not: which ones are ranked (titled by their map), and the
 * window a pairing has to be confirmed in.
 */
export function useRankedMatches(isSignedIn: boolean): RankedMatchFacts {
  const { data: ranked } = useQuery({ ...rankedOverviewQueryOptions(), enabled: isSignedIn });
  return useMemo(() => {
    if (!ranked) return NO_FACTS;
    const confirmDeadlines = new Map<string, number>();
    const rankedMaps = new Map<string, string>();
    for (const pool of ranked.pools) {
      for (const pairing of pool.pending) {
        confirmDeadlines.set(pairing.matchId, Date.parse(pairing.deadlineAt));
        rankedMaps.set(pairing.matchId, pairing.mapName);
      }
      for (const game of pool.inPlay) rankedMaps.set(game.matchId, game.mapName);
    }
    return { confirmDeadlines, rankedMaps };
  }, [ranked]);
}

/**
 * A clock for a list of deadlines, redrawn only as often as the nearest one
 * still ahead needs: a page of correspondence matches redraws once a minute,
 * and a live turn in its last minute ticks every second.
 */
export function useListNow(deadlines: readonly (number | null)[]): number {
  const [now, setNow] = useState(() => Date.now());
  const next = useMemo(
    () =>
      deadlines
        .filter((deadline): deadline is number => deadline !== null && deadline > now)
        .sort((left, right) => left - right)[0],
    [deadlines, now],
  );

  useEffect(() => {
    if (next === undefined) return;
    const timer = setTimeout(() => setNow(Date.now()), clockTickMs(next - now));
    return () => clearTimeout(timer);
  }, [next, now]);

  return now;
}

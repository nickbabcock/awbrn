/*
 * How a profile read turns the rows the database returns into the facts the
 * page shows.
 */

import { isProvisional } from "#/matchmaking/ranked_display.ts";
import type { MatchOutcome } from "#/matches/schemas.ts";
import type { ProfileCo, ProfileSeasonPlace } from "./schemas.ts";

/** How many COs the profile shows. */
const PROFILE_CO_COUNT = 3;

/**
 * The player's most played COs from their finished seats, grouped by CO and
 * army. Each CO stands on the army they field it with most.
 */
export function topCos(
  rows: readonly { coId: number; factionId: number; outcome: MatchOutcome; seats: number }[],
  count = PROFILE_CO_COUNT,
): ProfileCo[] {
  const byCo = new Map<number, ProfileCo & { games: number; armies: Map<number, number> }>();
  for (const row of rows) {
    let co = byCo.get(row.coId);
    if (!co) {
      co = {
        coId: row.coId,
        factionId: row.factionId,
        wins: 0,
        losses: 0,
        draws: 0,
        games: 0,
        armies: new Map(),
      };
      byCo.set(row.coId, co);
    }
    co.games += row.seats;
    if (row.outcome === "win") co.wins += row.seats;
    else if (row.outcome === "loss") co.losses += row.seats;
    else co.draws += row.seats;
    co.armies.set(row.factionId, (co.armies.get(row.factionId) ?? 0) + row.seats);
  }

  return [...byCo.values()]
    .sort(
      (left, right) => right.games - left.games || right.wins - left.wins || left.coId - right.coId,
    )
    .slice(0, count)
    .map(({ armies, coId, draws, losses, wins }) => {
      let factionId = 0;
      let most = -1;
      for (const [army, games] of armies) {
        if (games > most || (games === most && army < factionId)) {
          factionId = army;
          most = games;
        }
      }
      return { coId, factionId, wins, losses, draws };
    });
}

/**
 * Where the player stood in one pool at the close of each season from their
 * first rated match in it, and now in the open season.
 *
 * A closed season reads its frozen standings row. The open season reads the
 * live ladder, which does not reset between seasons, so a player placed on
 * it holds that place whether or not they have played since the season began.
 */
export function seasonPlaces({
  currentSeason,
  firstSeason,
  games,
  lastSeason,
  live,
  standings,
}: {
  currentSeason: number | null;
  firstSeason: number;
  games: ReadonlyMap<number, number>;
  lastSeason: number;
  live: { place: number | null; rating: number; deviation: number };
  standings: ReadonlyMap<
    number,
    { rank: number; rating: number; deviation: number; seasonMatches: number }
  >;
}): ProfileSeasonPlace[] {
  const places: ProfileSeasonPlace[] = [];
  for (let season = firstSeason; season <= lastSeason; season += 1) {
    if (season === currentSeason) {
      places.push({
        season,
        games: games.get(season) ?? 0,
        place: live.place,
        rating: live.rating,
        isProvisional: isProvisional(live.deviation),
      });
      continue;
    }
    const frozen = standings.get(season);
    places.push(
      frozen
        ? {
            season,
            games: frozen.seasonMatches,
            place: frozen.rank,
            rating: frozen.rating,
            isProvisional: isProvisional(frozen.deviation),
          }
        : {
            season,
            games: games.get(season) ?? 0,
            place: null,
            rating: null,
            isProvisional: false,
          },
    );
  }
  return places;
}

/** The most rating points a pool's string carries; a plot draws one per few pixels. */
const MAX_RATING_POINTS = 360;

/**
 * Thins a rating history (`[at, rating]`) to about `max` points, evenly by
 * match. The last point stays so the string ends where the rating stands, and
 * the highest and lowest stay so the string still reaches the peak's caret and
 * spans the row's scale.
 */
export function thinPoints(
  points: readonly [number, number][],
  max = MAX_RATING_POINTS,
): [number, number][] {
  if (points.length <= max) return [...points];
  let high = 0;
  let low = 0;
  for (const [index, [, rating]] of points.entries()) {
    if (rating > points[high]![1]) high = index;
    if (rating < points[low]![1]) low = index;
  }
  const stride = (points.length - 1) / (max - 1);
  const kept = new Set([high, low]);
  for (let index = 0; index < max; index += 1) kept.add(Math.round(index * stride));
  return [...kept].sort((left, right) => left - right).map((index) => points[index]!);
}

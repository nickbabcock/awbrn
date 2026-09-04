/*
 * What the ranked surfaces are allowed to say.
 *
 * Two rules from the design brief live here, because both are easy to break
 * with one careless line of JSX:
 *
 *   1. Show facts about the player, never facts about the pool. The ladder
 *      starts with very few players. A queue depth, a population count, or an
 *      estimated wait would read as an empty room, so this module offers no
 *      way to phrase one. Elapsed wait is a fact about the viewer and is
 *      allowed.
 *   2. Nothing about the opponent is known until both players commit. The
 *      redaction happens on the server; the copy here must agree with it.
 */

import type { RankedPool } from "#/matches/schemas.ts";
import { HARD_MAX_ACTIVE_MATCHES } from "./matchmaking.ts";

/**
 * The rating deviation above which a rating is not yet trusted.
 *
 * This governs how a rating reads, and nothing else. It puts the question mark
 * on a rating and it tells the hub the viewer is still finding their level.
 * Whether a player holds a place on the ladder is `LADDER_DEVIATION_LIMIT`,
 * and how far a rating moves is Glicko-2 alone.
 */
export const PROVISIONAL_DEVIATION = 150;

/**
 * The deviation above which a rating is too old to hold a ladder place.
 *
 * A player leaves the ladder a season after their last rated match, not a
 * season after the calendar turns. Removal is measured from the player's own
 * last match, so the ladder is never empty on the first day of a season and
 * players leave it one at a time.
 *
 * With `DEVIATION_GROWTH_PER_PERIOD`, a settled rating (deviation 50) reaches
 * this limit after about 90 days without a rated match. 150 is 30 days, 300 is
 * 131 days, and 350 is the 180 days that returns a rating to unrated.
 */
export const LADDER_DEVIATION_LIMIT = 250;

/**
 * How heavily the ladder counts uncertainty against a rating.
 *
 * The ladder is ordered by `rating - weight * deviation` and not by the rating
 * alone, so a player who stops playing slides down it instead of holding their
 * place until they vanish from it. A player far above the field stays near the
 * top while they slide, which is the honest report: the rating is still the
 * best guess at their strength, and it is only less certain than it was.
 *
 * A weight of 1 is about one place for each month away, at the spread this
 * ladder has. Glickman's own conservative estimate uses 2, which is correct
 * for a lower bound but drops an idle leader below the field within weeks.
 */
export const LADDER_DEVIATION_WEIGHT = 1;

/** The rating deviation given to a player with no rated match. */
export const MAXIMUM_DEVIATION = 350;

/** The length of one inactivity period, in milliseconds. */
export const DEVIATION_PERIOD_MS = 12 * 60 * 60 * 1000;

/**
 * The deviation added for each complete inactive period.
 *
 * The value moves a confirmed rating (deviation 50) back to the unrated
 * maximum after approximately 180 days without a rated match.
 */
export const DEVIATION_GROWTH_PER_PERIOD = 18.26;

/** The pools that accept a seek now. The other pools open later. */
export const OPEN_RANKED_POOLS: readonly RankedPool[] = ["async"];

/** The pools in the order that the pool tabs show them. */
export const RANKED_POOL_ORDER: readonly RankedPool[] = ["async", "fog_async", "live", "fog_live"];

export interface RankedPoolCopy {
  /** The tab label. */
  name: string;
  /** One line that says what a match in this pool is like. */
  summary: string;
  /** True when the player takes turns over days instead of in one sitting. */
  isAsync: boolean;
}

const POOL_COPY: Record<RankedPool, RankedPoolCopy> = {
  async: {
    name: "Async",
    summary: "Take your turns over days. Hold several games at once.",
    isAsync: true,
  },
  fog_async: {
    name: "Fog async",
    summary: "Fog of war, played over days. Hold several games at once.",
    isAsync: true,
  },
  live: {
    name: "Live",
    summary: "One game at a time, played in one sitting.",
    isAsync: false,
  },
  fog_live: {
    name: "Fog live",
    summary: "Fog of war, one game at a time, played in one sitting.",
    isAsync: false,
  },
};

export function rankedPoolCopy(pool: RankedPool): RankedPoolCopy {
  return POOL_COPY[pool];
}

export function isRankedPoolOpen(pool: RankedPool): boolean {
  return OPEN_RANKED_POOLS.includes(pool);
}

/**
 * The deviation to show, after the growth for time without a rated match.
 *
 * The growth uses complete 12-hour periods, and it stops while the player has
 * a rated match in progress. A player who is waiting for an opponent to move
 * has not gone inactive.
 */
export function readTimeDeviation(
  rating: { deviation: number; lastRatedAt: Date | null },
  now: Date,
  hasRatedMatchInProgress: boolean,
): number {
  if (hasRatedMatchInProgress || rating.lastRatedAt === null) {
    return Math.min(rating.deviation, MAXIMUM_DEVIATION);
  }

  const periods = Math.floor((now.getTime() - rating.lastRatedAt.getTime()) / DEVIATION_PERIOD_MS);
  if (periods <= 0) return Math.min(rating.deviation, MAXIMUM_DEVIATION);

  const grown = Math.sqrt(rating.deviation ** 2 + periods * DEVIATION_GROWTH_PER_PERIOD ** 2);
  return Math.min(grown, MAXIMUM_DEVIATION);
}

export function isProvisional(deviation: number): boolean {
  return deviation > PROVISIONAL_DEVIATION;
}

/** Whether a rating is recent enough to hold a place on the ladder. */
export function isListedOnLadder(deviation: number): boolean {
  return deviation <= LADDER_DEVIATION_LIMIT;
}

/**
 * What the ladder sorts by: the rating, less what is not known about it.
 *
 * The deviation must be the one time has grown, so that a rating which nobody
 * has tested lately gives up its place slowly.
 */
export function ladderScore(rating: number, deviation: number): number {
  return rating - LADDER_DEVIATION_WEIGHT * deviation;
}

/** A rating for display. A provisional rating carries a question mark. */
export function formatRating(rating: number, deviation: number): string {
  const value = Math.round(rating).toString();
  return isProvisional(deviation) ? `${value}?` : value;
}

export type SlotState = "in-play" | "searching" | "spare";

/**
 * The slot meter: one square for each of the five possible ranked games.
 *
 * A filled square is a game in play. The searching square is the slot that
 * the seek fills next. A spare square is capacity the player has not asked
 * for. The meter never shows how many other players are in the pool.
 */
export function slotMeter(input: {
  activeMatches: number;
  maxActiveMatches: number;
  isSeeking: boolean;
}): SlotState[] {
  const inPlay = Math.min(input.activeMatches, HARD_MAX_ACTIVE_MATCHES);
  const capacity = Math.min(Math.max(input.maxActiveMatches, inPlay), HARD_MAX_ACTIVE_MATCHES);

  return Array.from({ length: HARD_MAX_ACTIVE_MATCHES }, (_unused, index) => {
    if (index < inPlay) return "in-play";
    if (input.isSeeking && index < capacity) return "searching";
    return "spare";
  });
}

export type SeekWaitPhase = "searching" | "widened" | "unrestricted";

/** How far the rating range has opened for a seek that still waits. */
export function seekWaitPhase(createdAt: string, now: number): SeekWaitPhase {
  const hours = (now - Date.parse(createdAt)) / (60 * 60 * 1000);
  if (hours >= 24) return "unrestricted";
  if (hours >= 1) return "widened";
  return "searching";
}

/**
 * The one status line above the slot meter.
 *
 * Every branch describes the viewer's own seek. None of them describes the
 * pool.
 */
export function seekStatusLine(input: {
  isSeeking: boolean;
  activeMatches: number;
  maxActiveMatches: number;
  waitPhase: SeekWaitPhase;
  waitLabel: string;
}): string {
  // A pairing that waits for confirmation holds a slot as surely as a game
  // that is under way, so the count says slots rather than games.
  const taken = `${input.activeMatches} of ${input.maxActiveMatches} slots taken`;
  if (!input.isSeeking) {
    return input.activeMatches === 0 ? "Not seeking" : `Not seeking · ${taken}`;
  }

  if (input.activeMatches >= input.maxActiveMatches) {
    return `At capacity · ${taken}`;
  }

  const waited = `waiting ${input.waitLabel}`;
  switch (input.waitPhase) {
    case "widened":
      return `Seeking · ${taken} · ${waited} · widening the rating range`;
    case "unrestricted":
      return `Seeking · ${taken} · ${waited} · matching any rating`;
    default:
      return `Seeking · ${taken} · ${waited}`;
  }
}

/** The line under the slot meter, which says what happens next. */
export function capacityHelperLine(input: {
  isSeeking: boolean;
  activeMatches: number;
  maxActiveMatches: number;
}): string | null {
  if (!input.isSeeking) return null;
  if (input.activeMatches > input.maxActiveMatches) {
    const surplus = input.activeMatches - input.maxActiveMatches + 1;
    return `No new pairing arrives until ${describeGames(surplus)} ${surplus === 1 ? "ends" : "end"}.`;
  }
  if (input.activeMatches === input.maxActiveMatches) {
    return "A new pairing arrives when one of these ends.";
  }
  return null;
}

function describeGames(count: number): string {
  return count === 1 ? "1 game" : `${count} games`;
}

import type { RankedPool } from "#/matches/schemas.ts";

export const DEFAULT_MAX_ACTIVE_MATCHES = 3;
export const HARD_MAX_ACTIVE_MATCHES = 5;
export const INITIAL_RATING = 1500;
export const INITIAL_DEVIATION = 350;
export const UNRESTRICTED_AFTER_HOURS = 24;

const HOUR_MS = 60 * 60 * 1000;

export interface MatchmakingCandidate {
  userId: string;
  pool: RankedPool;
  generation: string;
  createdAt: Date;
  maxActiveMatches: number;
  activeMatches: number;
  rating: number;
  deviation: number;
}

export interface MatchmakingPair {
  first: MatchmakingCandidate;
  second: MatchmakingCandidate;
}

/** Compare user IDs with SQLite's default binary text ordering. */
export function compareUserIds(firstUserId: string, secondUserId: string): number {
  return firstUserId < secondUserId ? -1 : firstUserId > secondUserId ? 1 : 0;
}

/** Canonical key for a pair of distinct users. */
export function userPairKey(firstUserId: string, secondUserId: string): string {
  return compareUserIds(firstUserId, secondUserId) < 0
    ? `${firstUserId}\u0000${secondUserId}`
    : `${secondUserId}\u0000${firstUserId}`;
}

/** Whole hours for which a seek has waited. */
export function seekWaitHours(createdAt: Date, now: Date): number {
  return Math.max(0, Math.floor((now.getTime() - createdAt.getTime()) / HOUR_MS));
}

/** The rating difference this seek accepts against the given opponent. */
export function acceptedRatingDifference(
  seek: MatchmakingCandidate,
  opponent: MatchmakingCandidate,
  now: Date,
): number {
  const waitedHours = seekWaitHours(seek.createdAt, now);
  if (waitedHours >= UNRESTRICTED_AFTER_HOURS) return Number.POSITIVE_INFINITY;
  return 100 + seek.deviation + opponent.deviation + waitedHours * 100;
}

/** Whether both seeks accept one another now. */
export function candidatesAreCompatible(
  first: MatchmakingCandidate,
  second: MatchmakingCandidate,
  now: Date,
  activePairs: ReadonlySet<string>,
): boolean {
  if (first.userId === second.userId || first.pool !== second.pool) return false;
  if (
    first.activeMatches >= first.maxActiveMatches ||
    second.activeMatches >= second.maxActiveMatches
  ) {
    return false;
  }
  if (activePairs.has(userPairKey(first.userId, second.userId))) return false;

  const difference = Math.abs(first.rating - second.rating);
  return (
    difference <= acceptedRatingDifference(first, second, now) &&
    difference <= acceptedRatingDifference(second, first, now)
  );
}

/**
 * Pair the oldest seeks first and prefer the nearest rating for each seek.
 *
 * The input is not changed. Stable user-id tie breaks make retries select the
 * same pair when the database state has not changed.
 */
export function selectMatchmakingPairs(
  candidates: readonly MatchmakingCandidate[],
  now: Date,
  activePairs: ReadonlySet<string> = new Set(),
): MatchmakingPair[] {
  const remaining = [...candidates].sort(
    (left, right) =>
      left.createdAt.getTime() - right.createdAt.getTime() ||
      compareUserIds(left.userId, right.userId),
  );
  const pairs: MatchmakingPair[] = [];

  while (remaining.length > 1) {
    const first = remaining.shift()!;
    let bestIndex = -1;

    for (let index = 0; index < remaining.length; index += 1) {
      const candidate = remaining[index]!;
      if (!candidatesAreCompatible(first, candidate, now, activePairs)) continue;
      if (bestIndex === -1) {
        bestIndex = index;
        continue;
      }

      const best = remaining[bestIndex]!;
      const difference = Math.abs(first.rating - candidate.rating);
      const bestDifference = Math.abs(first.rating - best.rating);
      if (
        difference < bestDifference ||
        (difference === bestDifference &&
          (candidate.createdAt.getTime() < best.createdAt.getTime() ||
            (candidate.createdAt.getTime() === best.createdAt.getTime() &&
              compareUserIds(candidate.userId, best.userId) < 0)))
      ) {
        bestIndex = index;
      }
    }

    if (bestIndex === -1) continue;
    const [second] = remaining.splice(bestIndex, 1);
    pairs.push({ first, second: second! });
  }

  return pairs;
}

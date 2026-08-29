import { describe, expect, it } from "vitest";
import {
  acceptedRatingDifference,
  candidatesAreCompatible,
  selectMatchmakingPairs,
  type MatchmakingCandidate,
  userPairKey,
} from "./matchmaking.ts";

const now = new Date("2026-08-28T18:00:00.000Z");

function candidate(
  userId: string,
  overrides: Partial<MatchmakingCandidate> = {},
): MatchmakingCandidate {
  return {
    userId,
    pool: "async",
    generation: `seek-${userId}`,
    createdAt: now,
    maxActiveMatches: 3,
    activeMatches: 0,
    rating: 1500,
    deviation: 50,
    ...overrides,
  };
}

describe("ranked matchmaking", () => {
  it("uses both rating deviations in the initial range", () => {
    const first = candidate("a", { deviation: 70 });
    const second = candidate("b", { deviation: 80 });
    expect(acceptedRatingDifference(first, second, now)).toBe(250);
    expect(
      candidatesAreCompatible(
        first,
        candidate("b", { rating: 1750, deviation: 80 }),
        now,
        new Set(),
      ),
    ).toBe(true);
  });

  it("widens each seek by 100 points for each complete hour", () => {
    const old = candidate("a", { createdAt: new Date(now.getTime() - 3.5 * 60 * 60 * 1000) });
    expect(acceptedRatingDifference(old, candidate("b"), now)).toBe(500);
  });

  it("requires both seeks to accept the rating difference", () => {
    const old = candidate("a", {
      rating: 1000,
      deviation: 0,
      createdAt: new Date(now.getTime() - 24 * 60 * 60 * 1000),
    });
    const fresh = candidate("b", { rating: 1500, deviation: 0 });
    expect(acceptedRatingDifference(old, fresh, now)).toBe(Number.POSITIVE_INFINITY);
    expect(candidatesAreCompatible(old, fresh, now, new Set())).toBe(false);
  });

  it("removes both limits after both seeks wait 24 hours", () => {
    const createdAt = new Date(now.getTime() - 24 * 60 * 60 * 1000);
    expect(
      candidatesAreCompatible(
        candidate("a", { rating: 500, createdAt }),
        candidate("b", { rating: 2500, createdAt }),
        now,
        new Set(),
      ),
    ).toBe(true);
  });

  it("excludes users at capacity and an existing active pair", () => {
    const first = candidate("a");
    const second = candidate("b");
    expect(candidatesAreCompatible(first, { ...second, activeMatches: 3 }, now, new Set())).toBe(
      false,
    );
    expect(candidatesAreCompatible(first, second, now, new Set([userPairKey("b", "a")]))).toBe(
      false,
    );
  });

  it("takes the oldest seek and its closest compatible rating", () => {
    const pairs = selectMatchmakingPairs(
      [
        candidate("old", { rating: 1500, createdAt: new Date(now.getTime() - 60_000) }),
        candidate("far", { rating: 1650 }),
        candidate("near", { rating: 1510 }),
      ],
      now,
    );
    expect(pairs.map(({ first, second }) => [first.userId, second.userId])).toEqual([
      ["old", "near"],
    ]);
  });
});

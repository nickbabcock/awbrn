import { describe, expect, it } from "vitest";
import {
  DEVIATION_PERIOD_MS,
  MAXIMUM_DEVIATION,
  formatRating,
  isListedOnLadder,
  isProvisional,
  isRankedPoolOpen,
  leadingOwedPairingId,
  ladderScore,
  readTimeDeviation,
} from "./ranked_display.ts";
import { formatCompactDuration } from "#/utils/time.ts";

const now = Date.parse("2026-08-28T18:00:00.000Z");
const nowDate = new Date(now);

describe("readTimeDeviation", () => {
  it("keeps the stored deviation before one complete period", () => {
    const lastRatedAt = new Date(now - DEVIATION_PERIOD_MS + 1000);
    expect(readTimeDeviation({ deviation: 60, lastRatedAt }, nowDate, false)).toBe(60);
  });

  it("grows the deviation after one complete period", () => {
    const lastRatedAt = new Date(now - DEVIATION_PERIOD_MS);
    expect(readTimeDeviation({ deviation: 60, lastRatedAt }, nowDate, false)).toBeCloseTo(
      Math.sqrt(60 ** 2 + 18.26 ** 2),
      6,
    );
  });

  it("does not grow the deviation while a rated match is in progress", () => {
    const lastRatedAt = new Date(now - 40 * DEVIATION_PERIOD_MS);
    expect(readTimeDeviation({ deviation: 60, lastRatedAt }, nowDate, true)).toBe(60);
  });

  it("stops at the unrated maximum", () => {
    const lastRatedAt = new Date(now - 4000 * DEVIATION_PERIOD_MS);
    expect(readTimeDeviation({ deviation: 50, lastRatedAt }, nowDate, false)).toBe(
      MAXIMUM_DEVIATION,
    );
  });

  it("keeps the maximum for a player who has no rated match", () => {
    expect(readTimeDeviation({ deviation: 350, lastRatedAt: null }, nowDate, false)).toBe(350);
  });
});

describe("rating display", () => {
  it("marks a provisional rating with a question mark", () => {
    expect(isProvisional(151)).toBe(true);
    expect(isProvisional(150)).toBe(false);
    expect(formatRating(1500.4, true)).toBe("1500?");
    expect(formatRating(1512.6, false)).toBe("1513");
  });
});

describe("pool availability", () => {
  it("opens only the async pool for now", () => {
    expect(isRankedPoolOpen("async")).toBe(true);
    expect(isRankedPoolOpen("fog_async")).toBe(false);
    expect(isRankedPoolOpen("live")).toBe(false);
  });
});

describe("formatCompactDuration", () => {
  it("keeps two units at most", () => {
    expect(formatCompactDuration(45_000)).toBe("45s");
    expect(formatCompactDuration(28 * 60_000)).toBe("28m");
    expect(formatCompactDuration(14 * 3_600_000 + 2 * 60_000)).toBe("14h 02m");
    expect(formatCompactDuration(3 * 86_400_000 + 4 * 3_600_000)).toBe("3d 4h");
    expect(formatCompactDuration(-5)).toBe("0s");
  });
});

describe("the ladder place", () => {
  it("keeps a rating which is only a season old", () => {
    // 90 days at deviation 50 is what the limit is set to.
    const eightyDays = readTimeDeviation(
      { deviation: 50, lastRatedAt: new Date(0) },
      new Date(80 * 24 * 60 * 60 * 1000),
      false,
    );
    expect(isListedOnLadder(eightyDays)).toBe(true);
  });

  it("drops a rating which is older than a season", () => {
    const hundredDays = readTimeDeviation(
      { deviation: 50, lastRatedAt: new Date(0) },
      new Date(100 * 24 * 60 * 60 * 1000),
      false,
    );
    expect(isListedOnLadder(hundredDays)).toBe(false);
  });

  it("keeps a player who is above the field above it while they slide", () => {
    // The idle leader is 300 points clear, and a month away costs less.
    const idleLeader = ladderScore(2000, 130);
    const activeChaser = ladderScore(1700, 50);
    expect(idleLeader).toBeGreaterThan(activeChaser);
  });

  it("lets an active player pass an idle one they are close to", () => {
    const idle = ladderScore(1720, 200);
    const active = ladderScore(1700, 50);
    expect(active).toBeGreaterThan(idle);
  });

  it("counts nothing against a rating nobody doubts", () => {
    expect(ladderScore(1800, 0)).toBe(1800);
  });
});

describe("leadingOwedPairingId", () => {
  it("gives the red key to the first owed pairing only, and none when all are ready", () => {
    const ready = { matchId: "ready", isReady: true };
    const owed = [
      ready,
      { matchId: "first-owed", isReady: false },
      { matchId: "second-owed", isReady: false },
    ];
    expect(leadingOwedPairingId(owed)).toBe("first-owed");
    expect(leadingOwedPairingId([ready])).toBeNull();
  });
});

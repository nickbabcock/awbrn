import { describe, expect, it } from "vitest";
import {
  DEVIATION_PERIOD_MS,
  MAXIMUM_DEVIATION,
  capacityHelperLine,
  formatRating,
  isProvisional,
  isRankedPoolOpen,
  readTimeDeviation,
  seekStatusLine,
  seekWaitPhase,
  slotMeter,
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
    expect(formatRating(1500.4, 350)).toBe("1500?");
    expect(formatRating(1512.6, 80)).toBe("1513");
  });
});

describe("slotMeter", () => {
  it("fills the games in play and marks the slot the seek fills next", () => {
    expect(slotMeter({ activeMatches: 2, maxActiveMatches: 3, isSeeking: true })).toEqual([
      "in-play",
      "in-play",
      "searching",
      "spare",
      "spare",
    ]);
  });

  it("shows no searching slot at capacity", () => {
    expect(slotMeter({ activeMatches: 3, maxActiveMatches: 3, isSeeking: true })).toEqual([
      "in-play",
      "in-play",
      "in-play",
      "spare",
      "spare",
    ]);
  });

  it("shows no searching slot when the seek is stopped", () => {
    expect(slotMeter({ activeMatches: 1, maxActiveMatches: 4, isSeeking: false })).toEqual([
      "in-play",
      "spare",
      "spare",
      "spare",
      "spare",
    ]);
  });

  it("never hides a game in play below a lowered capacity", () => {
    expect(slotMeter({ activeMatches: 3, maxActiveMatches: 1, isSeeking: true })).toEqual([
      "in-play",
      "in-play",
      "in-play",
      "spare",
      "spare",
    ]);
  });
});

describe("seekWaitPhase", () => {
  it("widens after one hour and drops the limit after a day", () => {
    const at = (minutes: number) => new Date(now - minutes * 60_000).toISOString();
    expect(seekWaitPhase(at(59), now)).toBe("searching");
    expect(seekWaitPhase(at(60), now)).toBe("widened");
    expect(seekWaitPhase(at(24 * 60), now)).toBe("unrestricted");
  });
});

describe("seekStatusLine", () => {
  it("never states a fact about the pool", () => {
    const lines = [
      seekStatusLine({
        isSeeking: true,
        activeMatches: 2,
        maxActiveMatches: 3,
        waitPhase: "searching",
        waitLabel: "28m",
      }),
      seekStatusLine({
        isSeeking: true,
        activeMatches: 0,
        maxActiveMatches: 3,
        waitPhase: "unrestricted",
        waitLabel: "2d 4h",
      }),
      seekStatusLine({
        isSeeking: false,
        activeMatches: 0,
        maxActiveMatches: 3,
        waitPhase: "searching",
        waitLabel: "0s",
      }),
    ];

    for (const line of lines) {
      expect(line).not.toMatch(/player|opponent|queue|available|few|empty/i);
    }
  });

  it("reports capacity rather than a wait once every slot is in play", () => {
    expect(
      seekStatusLine({
        isSeeking: true,
        activeMatches: 3,
        maxActiveMatches: 3,
        waitPhase: "widened",
        waitLabel: "9h 00m",
      }),
    ).toBe("At capacity · 3 of 3 slots taken");
  });

  it("keeps a stopped seek honest about the games that continue", () => {
    expect(
      seekStatusLine({
        isSeeking: false,
        activeMatches: 1,
        maxActiveMatches: 3,
        waitPhase: "searching",
        waitLabel: "0s",
      }),
    ).toBe("Not seeking · 1 of 3 slots taken");
  });
});

describe("capacityHelperLine", () => {
  it("explains a capacity the player lowered below the games in play", () => {
    expect(capacityHelperLine({ isSeeking: true, activeMatches: 3, maxActiveMatches: 1 })).toBe(
      "No new pairing arrives until 3 games end.",
    );
  });

  it("explains the refill at capacity", () => {
    expect(capacityHelperLine({ isSeeking: true, activeMatches: 2, maxActiveMatches: 2 })).toBe(
      "A new pairing arrives when one of these ends.",
    );
  });

  it("says nothing while a slot is open", () => {
    expect(
      capacityHelperLine({ isSeeking: true, activeMatches: 1, maxActiveMatches: 3 }),
    ).toBeNull();
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

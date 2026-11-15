import { describe, expect, it } from "vitest";
import { layoutBunting, markSummary, nearestMark, ratingScale, seasonUnits } from "./bunting.ts";
import type { ProfilePool, ProfileSeason } from "./schemas.ts";

const DAY = 86_400_000;

/** Seasons of unequal length, so a test catches a scale that counts days. */
const seasons: ProfileSeason[] = [
  { number: 1, startsAt: 0, endsAt: 100 * DAY },
  { number: 2, startsAt: 100 * DAY, endsAt: 300 * DAY },
  { number: 3, startsAt: 300 * DAY, endsAt: 400 * DAY },
];

function pool(overrides: Partial<ProfilePool> = {}): ProfilePool {
  return {
    pool: "async",
    rating: 1600,
    isProvisional: false,
    place: 2,
    start: { at: 10 * DAY, rating: 1500 },
    points: [
      [50 * DAY, 1550],
      [200 * DAY, 1700],
      [350 * DAY, 1600],
    ],
    peak: { at: 200 * DAY, rating: 1700 },
    seasons: [
      { season: 1, games: 4, place: 7, rating: 1550, isProvisional: false },
      { season: 2, games: 0, place: null, rating: null, isProvisional: false },
      { season: 3, games: 2, place: 2, rating: 1600, isProvisional: true },
    ],
    ...overrides,
  };
}

describe("seasonUnits", () => {
  it("gives every season one unit whatever its length", () => {
    expect(seasonUnits(50 * DAY, seasons)).toBeCloseTo(0.5);
    expect(seasonUnits(200 * DAY, seasons)).toBeCloseTo(1.5);
    expect(seasonUnits(300 * DAY, seasons)).toBe(2);
  });

  it("runs before the first season and stops at the last close", () => {
    expect(seasonUnits(-10 * DAY, seasons)).toBeCloseTo(-0.1);
    expect(seasonUnits(900 * DAY, seasons)).toBe(3);
  });
});

describe("ratingScale", () => {
  it("keeps 1500 in view and puts the range on the string's band", () => {
    const y = ratingScale([1600, 1700]);
    expect(y(1700)).toBe(16);
    expect(y(1500)).toBe(50);
  });

  it("never draws a span narrower than 80 points", () => {
    const y = ratingScale([1510]);
    expect(y(1545)).toBe(16);
    expect(y(1465)).toBe(50);
  });
});

describe("layoutBunting", () => {
  const now = 375 * DAY;

  it("hangs each closed season's flag at its close and the open season's at the right edge", () => {
    const layout = layoutBunting({
      currentSeason: 3,
      now,
      openPools: ["async"],
      pools: [pool()],
      seasons,
    });
    const [row] = layout.rows;
    const xs = row!.marks.map((mark) => mark.x);
    // Equal columns from the lead-in to the last close.
    expect(xs[2]).toBe(100);
    expect(xs[1]! - xs[0]!).toBeCloseTo(xs[2]! - xs[1]!);
    expect(row!.marks.map((mark) => mark.kind)).toEqual(["flag", "hook", "flag"]);
    expect(row!.marks.map((mark) => mark.isOpen)).toEqual([false, false, true]);
    // The axis starts a little before the first rated match, at the pin.
    expect(row!.pin.x).toBe(0);
    expect(layout.axis.map((label) => label.x)).toEqual(xs);
  });

  it("starts the open season's wash at the last closed season's flag", () => {
    const layout = layoutBunting({
      currentSeason: 3,
      now,
      openPools: [],
      pools: [pool()],
      seasons,
    });
    expect(layout.washFrom).toBeCloseTo(layout.rows[0]!.marks[1]!.x);
  });

  it("has no wash or dotted string between seasons", () => {
    const layout = layoutBunting({
      currentSeason: null,
      now,
      openPools: [],
      pools: [pool()],
      seasons,
    });
    expect(layout.washFrom).toBeNull();
    expect(layout.rows[0]!.aheadPath).toBeNull();
    expect(layout.rows[0]!.marks.every((mark) => !mark.isOpen)).toBe(true);
  });

  it("shows the peak only once the string has come down from it", () => {
    const down = layoutBunting({ currentSeason: 3, now, openPools: [], pools: [pool()], seasons });
    expect(down.rows[0]!.peak?.rating).toBe(1700);

    const atPeak = layoutBunting({
      currentSeason: 3,
      now,
      openPools: [],
      pools: [pool({ peak: { at: 350 * DAY, rating: 1600 } })],
      seasons,
    });
    expect(atPeak.rows[0]!.peak).toBeNull();
  });

  it("lines every pool up on one axis from the earliest first match", () => {
    const later = pool({
      pool: "fog_async",
      start: { at: 150 * DAY, rating: 1500 },
      points: [[250 * DAY, 1520]],
      seasons: [
        { season: 2, games: 1, place: 12, rating: 1520, isProvisional: true },
        { season: 3, games: 0, place: null, rating: null, isProvisional: false },
      ],
    });
    const layout = layoutBunting({
      currentSeason: 3,
      now,
      openPools: [],
      pools: [pool(), later],
      seasons,
    });
    const [first, second] = layout.rows;
    expect(second!.pin.x).toBeGreaterThan(first!.pin.x);
    expect(second!.marks[0]!.x).toBeCloseTo(first!.marks[1]!.x);
    expect(second!.marks[0]!.band).toBe("low");
    // Off the ladder in the open season: an unraised flag under the batten, not a hook.
    expect(second!.marks[1]).toMatchObject({ kind: "unraised", isOpen: true });
  });

  it("hangs a closed season's flag where the drawn string crosses its close", () => {
    const layout = layoutBunting({
      currentSeason: 3,
      now,
      openPools: [],
      pools: [pool()],
      seasons,
    });
    const y = ratingScale([1500, 1550, 1700, 1600]);
    // S1 closes halfway between the moves at 0.5 and 1.5 seasons.
    expect(layout.rows[0]!.marks[0]!.y).toBeCloseTo((y(1550) + y(1700)) / 2);
  });

  it("keeps the start pin clear of the first flag", () => {
    const late = pool({
      pool: "live",
      start: { at: 295 * DAY, rating: 1500 },
      points: [[297 * DAY, 1516]],
      seasons: [{ season: 2, games: 1, place: 4, rating: 1516, isProvisional: true }],
    });
    const layout = layoutBunting({
      currentSeason: 3,
      now,
      openPools: [],
      pools: [pool(), late],
      seasons,
    });
    const row = layout.rows[1]!;
    expect(row.pin.x).toBeCloseTo(row.marks[0]!.x - 7);
  });

  it("thins dense play to a point per half percent of the plot, ending on the rating", () => {
    const dense = Array.from({ length: 1000 }, (_, index): [number, number] => [
      300 * DAY + index * 0.07 * DAY,
      index % 2 ? 1600 : 1500,
    ]);
    const layout = layoutBunting({
      currentSeason: 3,
      now,
      openPools: [],
      pools: [pool({ points: dense, peak: { at: dense[1]![0], rating: 1600 } })],
      seasons,
    });
    const moves = layout.rows[0]!.stringPath.split(/[ML]/)
      .filter(Boolean)
      .map((move) => move.trim().split(" ").map(Number));
    // The pin, about 47 points over the 23.6% of the plot the games span, and the run to now.
    expect(moves.length).toBeLessThan(55);
    // The last game is kept, so the level run to now starts from it.
    expect(moves.at(-2)![1]).toBeCloseTo(moves.at(-1)![1]!);
  });

  it("drops every other label on a phone from the open season back, once the seasons are dense", () => {
    const many = Array.from({ length: 9 }, (_, index) => ({
      number: index + 1,
      startsAt: index * 100 * DAY,
      endsAt: (index + 1) * 100 * DAY,
    }));
    const layout = layoutBunting({
      currentSeason: 9,
      now: 850 * DAY,
      openPools: [],
      pools: [pool({ seasons: [] })],
      seasons: many,
    });
    expect(layout.isDense).toBe(true);
    expect(layout.axis.at(-1)!.isAlternate).toBe(false);
    expect(layout.axis.map((label) => label.isAlternate)).toEqual([
      false,
      true,
      false,
      true,
      false,
      true,
      false,
      true,
      false,
    ]);
  });

  it("widens the plot past twelve seasons of play, so it scrolls", () => {
    const many = (count: number) =>
      Array.from({ length: count }, (_, index) => ({
        number: index + 1,
        startsAt: index * 100 * DAY,
        endsAt: (index + 1) * 100 * DAY,
      }));
    const layout = (count: number, firstAt: number) =>
      layoutBunting({
        currentSeason: count,
        now: (count - 0.5) * 100 * DAY,
        openPools: [],
        pools: [pool({ start: { at: firstAt, rating: 1500 }, seasons: [] })],
        seasons: many(count),
      });
    expect(layout(12, 10 * DAY).scrollWidth).toBe(1);
    // Twenty-four seasons from the first match: twice the width in view.
    expect(layout(24, 0).scrollWidth).toBeCloseTo((24 + 0.06) / 12);
    // Seasons before the first match are not drawn, so they never widen it.
    expect(layout(16, 500 * DAY).scrollWidth).toBe(1);
  });

  it("gives an unrated player one slack row per open pool, ending at the open season", () => {
    const layout = layoutBunting({
      currentSeason: 3,
      now,
      openPools: ["async"],
      pools: [],
      seasons: [seasons[2]!],
    });
    expect(layout.rows).toHaveLength(1);
    expect(layout.rows[0]!.isUnrated).toBe(true);
    expect(layout.rows[0]!.marks).toMatchObject([{ kind: "unraised", isOpen: true, x: 100 }]);
    expect(layout.axis).toEqual([{ season: 3, x: 100, isAlternate: false }]);
  });
});

describe("nearestMark", () => {
  it("picks the season whose column the pointer is in, half-gaps included", () => {
    const marks = [{ x: 20 }, { x: 60 }, { x: 100 }];
    expect(nearestMark(marks, 0)).toBe(0);
    expect(nearestMark(marks, 39)).toBe(0);
    expect(nearestMark(marks, 41)).toBe(1);
    expect(nearestMark(marks, 95)).toBe(2);
  });
});

describe("markSummary", () => {
  const format = (rating: number, isProvisional: boolean) =>
    `${Math.round(rating)}${isProvisional ? "?" : ""}`;
  const base = {
    season: 3,
    band: "pod" as const,
    isGold: true,
    isOpen: false,
    x: 0,
    y: 0,
  };

  it("reads a flag as place, rating and games", () => {
    expect(
      markSummary(
        { ...base, kind: "flag", place: 1, rating: 1731.4, games: 31, isProvisional: false },
        format,
      ),
    ).toBe("#1 · 1731 · 31 games");
  });

  it("says no place, or no games, rather than a zero", () => {
    expect(
      markSummary(
        { ...base, kind: "unraised", place: null, rating: null, games: 1, isProvisional: false },
        format,
      ),
    ).toBe("No place · 1 game");
    expect(
      markSummary(
        { ...base, kind: "unraised", place: null, rating: null, games: 0, isProvisional: false },
        format,
      ),
    ).toBe("No games");
  });
});

/*
 * The bunting's geometry: where each pool's string runs and where each
 * season's mark hangs, as percentages of the plot, so the page draws it at
 * any width without measuring.
 *
 * Time runs in season units: season `i` spans `[i, i + 1)` whatever its
 * length in days, so every season gets the same width and a flag hangs at
 * each whole unit, the season's close.
 */

import type { RankedPool } from "#/matches/schemas.ts";
import type { ProfilePool, ProfileSeason } from "./schemas.ts";

/** How far before the first rated match the axis starts, in seasons. */
const LEAD_IN = 0.06;
/** The rating every player starts from; each row's scale keeps it in view. */
const START_RATING = 1500;
/** The narrowest rating span a row draws, so a few points never look like a cliff. */
const MIN_RATING_SPAN = 80;
/** The band of the plot the string may use (percent from the top); flags hang below. */
const STRING_TOP = 16;
const STRING_BOTTOM = 50;
/** From this many seasons the flags narrow and a phone labels every other season. */
const DENSE_SEASONS = 9;
/** Past this many seasons the plot scrolls sideways rather than squeeze its flags. */
const VISIBLE_SEASONS = 12;
/** The string keeps a point per this much of the plot: about 3px on a desktop row. */
const POINT_PITCH = 0.5;
/** How far the start pin stays before the first flag, so the flag never covers it. */
const PIN_CLEARANCE = 7;
/** An unrated row: the pin and the flag at this height, the string hanging below them. */
const SLACK_Y = 24;

/** How long a flag hangs: its place band. */
export type FlagBand = "pod" | "mid" | "low";

export type MarkKind = "flag" | "unraised" | "hook";

/** One season's mark on one pool's string. */
export interface BuntingMark {
  season: number;
  kind: MarkKind;
  band: FlagBand;
  /** First place: the only gold flag. */
  isGold: boolean;
  isProvisional: boolean;
  /** The open season, still flying. */
  isOpen: boolean;
  place: number | null;
  rating: number | null;
  games: number;
  /** Percent of the plot's width and height. */
  x: number;
  y: number;
}

export interface BuntingRow {
  pool: RankedPool;
  rating: number | null;
  isProvisional: boolean;
  place: number | null;
  /** No rating in the pool: the string hangs slack to the open season's flag. */
  isUnrated: boolean;
  pin: { x: number; y: number };
  /** The string in a 1000 × 100 box (`preserveAspectRatio="none"`). */
  stringPath: string;
  /** Dotted, from now to the open season's close. */
  aheadPath: string | null;
  /** `season` is the one the peak was reached in. */
  peak: { x: number; y: number; rating: number; season: number } | null;
  marks: BuntingMark[];
}

export interface BuntingAxisLabel {
  season: number;
  x: number;
  /** Dropped on a phone when the seasons are dense; the open season always keeps its label. */
  isAlternate: boolean;
}

export interface BuntingLayout {
  isDense: boolean;
  /** The plot's width over the width in view: past twelve seasons it is more than 1, and scrolls. */
  scrollWidth: number;
  /** Where the open season's wash starts: the last closed season's flag. Null with none. */
  washFrom: number | null;
  axis: BuntingAxisLabel[];
  rows: BuntingRow[];
}

/** The place band: podium, the top ten, and everyone below. */
function flagBand(place: number): FlagBand {
  if (place <= 3) return "pod";
  if (place <= 10) return "mid";
  return "low";
}

/** A time (epoch ms) in season units: season `i` of the list spans `[i, i + 1)`. */
export function seasonUnits(at: number, seasons: readonly ProfileSeason[]): number {
  const first = seasons[0];
  if (!first) return 0;
  if (at < first.startsAt) return (at - first.startsAt) / (first.endsAt - first.startsAt);
  for (const [index, season] of seasons.entries()) {
    if (at < season.endsAt) {
      return index + Math.max(0, at - season.startsAt) / (season.endsAt - season.startsAt);
    }
  }
  return seasons.length;
}

/** A row's rating scale: its range with 1500 in it, at least `MIN_RATING_SPAN` points tall. */
export function ratingScale(ratings: readonly number[]): (rating: number) => number {
  let low = Math.min(START_RATING, ...ratings);
  let high = Math.max(START_RATING, ...ratings);
  if (high - low < MIN_RATING_SPAN) {
    const middle = (high + low) / 2;
    low = middle - MIN_RATING_SPAN / 2;
    high = middle + MIN_RATING_SPAN / 2;
  }
  return (rating) => STRING_TOP + ((high - rating) / (high - low)) * (STRING_BOTTOM - STRING_TOP);
}

/**
 * Thins the string to a point per `POINT_PITCH` of the plot, so dense play
 * never scribbles. The last point stays, so the string ends where the rating
 * stands, and so does the peak, so its caret sits on the string.
 */
function thinByX<T extends { x: number }>(points: readonly T[], keep: (point: T) => boolean): T[] {
  const kept: T[] = [];
  for (const [index, point] of points.entries()) {
    const last = kept[kept.length - 1];
    const isClose = last !== undefined && point.x - last.x < POINT_PITCH;
    if (index === points.length - 1) {
      // The end replaces a point too close to it rather than jagging beside it.
      if (isClose && kept.length > 1 && !keep(last)) kept.pop();
      kept.push(point);
    } else if (!isClose || keep(point)) {
      kept.push(point);
    }
  }
  return kept;
}

/** Where the drawn string crosses `x`, so a flag hangs from the string itself. */
function stringY(line: readonly { x: number; y: number }[], x: number): number {
  let previous = line[0]!;
  for (const point of line) {
    if (point.x >= x) {
      if (point.x === previous.x) return point.y;
      return previous.y + ((x - previous.x) / (point.x - previous.x)) * (point.y - previous.y);
    }
    previous = point;
  }
  return previous.y;
}

function pathFrom(points: readonly { x: number; y: number }[]): string {
  return points
    .map(
      (point, index) =>
        `${index === 0 ? "M" : "L"}${(point.x * 10).toFixed(1)} ${point.y.toFixed(2)}`,
    )
    .join(" ");
}

export function layoutBunting({
  currentSeason,
  now,
  openPools,
  pools,
  seasons,
}: {
  currentSeason: number | null;
  now: number;
  openPools: readonly RankedPool[];
  pools: readonly ProfilePool[];
  seasons: readonly ProfileSeason[];
}): BuntingLayout {
  const count = seasons.length;
  const isDense = count >= DENSE_SEASONS;
  const hasOpenSeason = currentSeason !== null && seasons[count - 1]?.number === currentSeason;
  const axis = seasons.map((season, index) => ({
    season: season.number,
    x: 0,
    isAlternate: isDense && (count - 1 - index) % 2 === 1,
    index,
  }));

  if (pools.length === 0) {
    // One open season with nothing played: the pin, a slack string, the season's flag.
    const season = seasons[count - 1];
    return {
      isDense: false,
      scrollWidth: 1,
      washFrom: null,
      axis: season ? [{ season: season.number, x: 100, isAlternate: false }] : [],
      rows: season
        ? openPools.map((pool) => ({
            pool,
            rating: null,
            isProvisional: false,
            place: null,
            isUnrated: true,
            pin: { x: 0, y: SLACK_Y },
            stringPath: `M0 ${SLACK_Y} Q500 78 1000 ${SLACK_Y}`,
            aheadPath: null,
            peak: null,
            marks: [
              {
                season: season.number,
                kind: "unraised",
                band: "low",
                isGold: false,
                isProvisional: false,
                isOpen: hasOpenSeason,
                place: null,
                rating: null,
                games: 0,
                x: 100,
                y: SLACK_Y,
              },
            ],
          }))
        : [],
    };
  }

  const startUnits =
    Math.min(...pools.map((pool) => seasonUnits(pool.start.at, seasons))) - LEAD_IN;
  const x = (units: number) => ((units - startUnits) / (count - startUnits)) * 100;
  const nowX = x(seasonUnits(now, seasons));

  const rows = pools.map((pool): BuntingRow => {
    const y = ratingScale([pool.start.rating, ...pool.points.map(([, rating]) => rating)]);
    const marked = pool.seasons.flatMap((entry) => {
      const index = seasons.findIndex((season) => season.number === entry.season);
      return index < 0 ? [] : [{ entry, index }];
    });
    const firstFlagX = marked[0] ? x(marked[0].index + 1) : 100;
    const pin = {
      x: Math.max(
        0,
        Math.min(x(seasonUnits(pool.start.at, seasons) - LEAD_IN), firstFlagX - PIN_CLEARANCE),
      ),
      y: y(pool.start.rating),
    };
    const last = pool.points[pool.points.length - 1]?.[1] ?? pool.start.rating;
    const line = [
      pin,
      ...thinByX(
        pool.points.map(([at, rating]) => ({ at, x: x(seasonUnits(at, seasons)), y: y(rating) })),
        (point) => point.at === pool.peak.at,
      ),
    ];
    // The string runs on, level, to now; only what is still to come is dotted.
    if (hasOpenSeason) line.push({ x: nowX, y: y(last) });

    const marks = marked.map(({ entry, index }): BuntingMark => {
      const isOpen = hasOpenSeason && index === count - 1;
      // A frozen season with no games has an empty hook; the open season
      // still flies an unraised flag under its batten.
      const kind = entry.place !== null ? "flag" : entry.games > 0 || isOpen ? "unraised" : "hook";
      const markX = x(index + 1);
      return {
        season: entry.season,
        kind,
        // An unraised flag is the short pocket: no place, so no band to claim.
        band: entry.place === null ? "low" : flagBand(entry.place),
        isGold: entry.place === 1,
        isProvisional: kind === "flag" && entry.isProvisional,
        isOpen,
        place: entry.place,
        rating: entry.rating,
        games: entry.games,
        x: markX,
        y: isOpen ? y(last) : stringY(line, markX),
      };
    });

    // The peak earns its caret once the string has come down from it.
    const peakUnits = seasonUnits(pool.peak.at, seasons);
    const peak =
      Math.round(pool.peak.rating) > Math.round(last)
        ? {
            x: x(peakUnits),
            y: y(pool.peak.rating),
            rating: Math.round(pool.peak.rating),
            season: seasons[Math.min(count - 1, Math.max(0, Math.floor(peakUnits)))]!.number,
          }
        : null;

    return {
      pool: pool.pool,
      rating: pool.rating,
      isProvisional: pool.isProvisional,
      place: pool.place,
      isUnrated: false,
      pin,
      stringPath: pathFrom(line),
      aheadPath: hasOpenSeason
        ? pathFrom([
            { x: nowX, y: y(last) },
            { x: 100, y: y(last) },
          ])
        : null,
      peak,
      marks,
    };
  });

  return {
    isDense,
    scrollWidth: Math.max(1, (count - startUnits) / VISIBLE_SEASONS),
    washFrom: hasOpenSeason && count > 1 ? x(count - 1) : null,
    axis: axis.map(({ index, isAlternate, season }) => ({ season, isAlternate, x: x(index + 1) })),
    rows,
  };
}

/** The season a pointer at `x` percent picks: the nearest mark, so a column's half-gaps count. */
export function nearestMark(marks: readonly { x: number }[], x: number): number {
  let best = 0;
  for (const [index, mark] of marks.entries()) {
    if (Math.abs(mark.x - x) < Math.abs(marks[best]!.x - x)) best = index;
  }
  return best;
}

/** `#1 · 1731 · 31 games`, `No place · 12 games`, or `No games`. */
export function markSummary(
  mark: BuntingMark,
  formatRating: (rating: number, isProvisional: boolean) => string,
): string {
  const games = `${mark.games} ${mark.games === 1 ? "game" : "games"}`;
  switch (mark.kind) {
    case "flag":
      return [
        `#${mark.place}`,
        mark.rating === null ? null : formatRating(mark.rating, mark.isProvisional),
        games,
      ]
        .filter((part) => part !== null)
        .join(" · ");
    case "unraised":
      // An unrated player's waiting flag has no games behind it.
      return mark.games > 0 ? `No place · ${games}` : "No games";
    case "hook":
      return "No games";
  }
}

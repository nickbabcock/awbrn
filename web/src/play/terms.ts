import {
  clockPressure,
  findClockPreset,
  formatClockDuration,
  formatClockTerms,
} from "#/matches/match_clock.ts";
import type { MatchClock } from "#/matches/schemas.ts";

/**
 * A match's clock as the figure the lists show: the pace's name and its
 * terms in one mono run (`Live 5m +2m`, `Async 7d +2d`), or the terms alone
 * when nobody named them (`3d +1d`).
 */
export function clockTerms(clock: MatchClock): string {
  const preset = findClockPreset(clock);
  const terms = formatClockTerms(clock);
  return preset ? `${preset.name} ${terms}` : terms;
}

/** Seats as a figure: `1 / 2`. */
export function seatsFigure(taken: number, total: number): string {
  return `${taken} / ${total}`;
}

/**
 * Time left as the figure a row shows (`3m 42s`, `2d 6h`), or `0s` once the
 * deadline has passed. Lists name an overdue turn with the figure and its
 * accessible label rather than a word in the figure's place.
 */
export function timeLeftFigure(remainingMs: number): string {
  return formatClockDuration(Math.max(0, remainingMs));
}

/** What a screen reader hears for a time-left figure. */
export function timeLeftLabel(remainingMs: number): string {
  return remainingMs <= 0 ? "Overdue" : `${formatClockDuration(remainingMs)} left`;
}

/**
 * Whether a running turn clock reads as a warning: low against the bank the
 * match opened on. A clock running low is a warning, never red.
 */
export function isClockLow(remainingMs: number, clock: MatchClock): boolean {
  return clockPressure(remainingMs, clock.initialMs) !== "steady";
}

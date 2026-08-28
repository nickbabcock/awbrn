import type { GameCommand } from "#/wasm/awbrn_server.js";
import { MAX_CLOCK_MS, type MatchClock } from "./schemas.ts";

/**
 * One recorded action, reduced to what the clock charges for.
 *
 * Only the turn boundaries matter: a seat is charged for the whole span from
 * the moment its turn opened to the moment it closed it, whatever it did in
 * between.
 */
export interface ClockAction {
  slotIndex: number;
  /** True for the commands that close a turn: `endTurn` and `timeout`. */
  endsTurn: boolean;
  /** When the durable object recorded the action, in milliseconds. */
  at: number;
}

/** What the clock reads for a match that is still being played. */
export interface MatchClockState {
  /** Time left for each seat, by slot index. */
  banksMs: Record<number, number>;
  /** When the open turn started. */
  turnStartedAt: number;
  /** The seat the open turn belongs to. */
  activeSlot: number;
  /** When the active seat runs out and is removed from the match. */
  deadlineAt: number;
}

/** The commands that close a turn, and so settle a seat's bank. */
export function commandEndsTurn(command: GameCommand): boolean {
  return command.type === "endTurn" || command.type === "timeout";
}

/**
 * What the recorded actions have charged so far, and the turn they left open.
 *
 * The clock is a fold over the action log, and this is the fold's running
 * total: a caller that keeps one feeds it each new action as it is recorded
 * and pays for the action rather than for the whole history. A caller that has
 * lost it rebuilds the same numbers with `computeMatchClock`.
 */
export interface ClockProgress {
  readonly clock: MatchClock;
  /** Time left for each seat, by slot index, as the closed turns leave it. */
  banksMs: Record<number, number>;
  /** When the open turn started. */
  turnStartedAt: number;
  /** The seat whose turn is open, or null between turns. */
  openSlot: number | null;
  /** When the last action was recorded. */
  lastActionAt: number;
}

/** The clock as a match stands at its first turn, before any action. */
export function startClockProgress(
  clock: MatchClock,
  matchStartedAt: number,
  slotCount: number,
): ClockProgress {
  const banksMs: Record<number, number> = {};
  // A seat that has not closed a turn yet still has to appear in the banks, or
  // the screen reads it as out of time for the whole first round.
  for (let slotIndex = 0; slotIndex < slotCount; slotIndex += 1) {
    banksMs[slotIndex] = clock.initialMs;
  }
  return {
    clock,
    banksMs,
    turnStartedAt: matchStartedAt,
    openSlot: null,
    lastActionAt: matchStartedAt,
  };
}

function bankOf(progress: ClockProgress, slotIndex: number): number {
  return progress.banksMs[slotIndex] ?? progress.clock.initialMs;
}

/** Charge a seat for its turn, then give it the increment back. */
function settle(progress: ClockProgress, slotIndex: number, closedAt: number): void {
  // A clock that moves backwards would refund time and would move the next
  // turn's deadline in with it, so a timestamp older than the turn it closes
  // is read as the moment that turn opened.
  const closed = Math.max(progress.turnStartedAt, closedAt);
  const elapsed = closed - progress.turnStartedAt;
  const remaining = bankOf(progress, slotIndex) - elapsed;
  // A seat that ran out is leaving the match, so it earns no increment.
  progress.banksMs[slotIndex] =
    remaining > 0 ? Math.min(remaining + progress.clock.incrementMs, progress.clock.maxBankMs) : 0;
  progress.turnStartedAt = closed;
  progress.openSlot = null;
}

/** Charge the clock for one recorded action. `action` must be the next one. */
export function advanceClockProgress(progress: ClockProgress, action: ClockAction): void {
  // A seat can lose its turn without a command that says so, by routing itself
  // on its own turn. The engine passes play on at that action, so the seat is
  // charged up to it and no further.
  if (progress.openSlot !== null && progress.openSlot !== action.slotIndex) {
    settle(progress, progress.openSlot, progress.lastActionAt);
  }
  progress.openSlot = action.slotIndex;
  progress.lastActionAt = action.at;
  if (action.endsTurn) {
    settle(progress, action.slotIndex, action.at);
  }
}

/**
 * Read the clock, with the seat the engine says is on the move.
 *
 * The same handover, when it is the last thing the match recorded, is read
 * here rather than charged: the seat that lost its turn keeps the charge only
 * once a further action confirms it, which `advanceClockProgress` then makes.
 */
export function readClockProgress(progress: ClockProgress, activeSlot: number): MatchClockState {
  const read: ClockProgress = { ...progress, banksMs: { ...progress.banksMs } };
  if (read.openSlot !== null && read.openSlot !== activeSlot) {
    settle(read, read.openSlot, read.lastActionAt);
  }

  return {
    banksMs: read.banksMs,
    turnStartedAt: read.turnStartedAt,
    activeSlot,
    deadlineAt: read.turnStartedAt + bankOf(read, activeSlot),
  };
}

/**
 * Read every seat's clock from the recorded actions.
 *
 * The event log is the only durable state a match has, so the clock is derived
 * from it rather than kept beside it: a durable object that is evicted and
 * woken rebuilds the same numbers it had before. `actions` must be in the
 * order they were recorded.
 */
export function computeMatchClock(
  clock: MatchClock,
  matchStartedAt: number,
  actions: readonly ClockAction[],
  activeSlot: number,
  slotCount: number,
): MatchClockState {
  const progress = startClockProgress(clock, matchStartedAt, slotCount);
  for (const action of actions) {
    advanceClockProgress(progress, action);
  }
  return readClockProgress(progress, activeSlot);
}

/** Time the active seat has left, floored at zero. */
export function remainingMs(state: MatchClockState, now: number): number {
  return Math.max(0, state.deadlineAt - now);
}

/**
 * A span of time as a player reads it: the two largest units that carry it.
 *
 * A seven day bank does not need its minutes, and a seat with two minutes left
 * does not want to read them as a fraction of a day.
 */
export function formatClockDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  if (minutes > 0) return seconds > 0 ? `${minutes}m ${seconds}s` : `${minutes}m`;
  return `${seconds}s`;
}

/**
 * A span read to the second, the way a clock face gives it.
 *
 * This is the reading a player asks for when they open the clock rather than
 * glance at it, so it keeps every unit from the largest down to the seconds
 * and pads each one after the first, which is what stops the digits jumping
 * about as they count down.
 */
export function formatClockCountdown(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (value: number) => String(value).padStart(2, "0");

  if (days > 0) return `${days}d ${pad(hours)}h ${pad(minutes)}m ${pad(seconds)}s`;
  if (hours > 0) return `${hours}h ${pad(minutes)}m ${pad(seconds)}s`;
  if (minutes > 0) return `${minutes}m ${pad(seconds)}s`;
  return `${seconds}s`;
}

/**
 * The clock's terms alone: the starting bank, what a turn gives back, and the
 * ceiling when it is not the bank the match started on. `7d +2d`.
 */
export function formatClockTerms(clock: MatchClock): string {
  const terms = `${formatClockDuration(clock.initialMs)} +${formatClockDuration(clock.incrementMs)}`;
  return clock.maxBankMs === clock.initialMs || isBankUncapped(clock)
    ? terms
    : `${terms}, up to ${formatClockDuration(clock.maxBankMs)}`;
}

/**
 * The clock in one line, as a match listing shows it.
 *
 * A pace with a name leads with it, because a player scanning a board of
 * matches is looking for the one they have time to play, and `Campaign`
 * answers that before `7d +2d` does. Terms nobody named read as terms alone.
 */
export function formatClockSummary(clock: MatchClock): string {
  const terms = formatClockTerms(clock);
  const preset = findClockPreset(clock);
  return preset ? `${preset.name} · ${terms}` : terms;
}

/**
 * How often a running clock has to redraw to stay honest.
 *
 * `formatClockDuration` prints the two largest units it has, so a bank with
 * days on it never shows a second and does not have to be redrawn for one. A
 * correspondence match left open in a tab costs one frame a minute instead of
 * one a second, and the last minute of a live match still ticks.
 */
export function clockTickMs(remaining: number): number {
  if (remaining < HOUR_MS) return 1_000;
  if (remaining < DAY_MS) return 15_000;
  return MINUTE_MS;
}

/** One minute, one hour, and one day, in milliseconds, as the clock's own units. */
export const MINUTE_MS = 60_000;
export const HOUR_MS = 60 * MINUTE_MS;
export const DAY_MS = 24 * HOUR_MS;

/** A time control a host can take without setting three numbers. */
export interface ClockPreset {
  id: string;
  /** What the preset is called on the create screen and in a match listing. */
  name: string;
  /** Who the pace is for, in one line. */
  brief: string;
  clock: MatchClock;
}

/**
 * The two paces a match is actually played at.
 *
 * A host is choosing between a match played at the board and a match played
 * over days, not three durations. Everything else is a variation on one of
 * those two, and a host who wants one writes their own terms.
 */
export const CLOCK_PRESETS: readonly ClockPreset[] = [
  {
    id: "live",
    name: "Live",
    brief: "One sitting.",
    clock: { initialMs: 5 * MINUTE_MS, incrementMs: 2 * MINUTE_MS, maxBankMs: MAX_CLOCK_MS },
  },
  {
    id: "async",
    name: "Async",
    brief: "A turn a day.",
    clock: { initialMs: 7 * DAY_MS, incrementMs: 2 * DAY_MS, maxBankMs: 7 * DAY_MS },
  },
];

/**
 * True for a bank nothing holds back.
 *
 * Every clock names a ceiling because the rules need one, but a ceiling set to
 * the highest the system takes is a ceiling no match reaches, and printing it
 * would report a limit that never binds as though it were a term of the game.
 */
export function isBankUncapped(clock: MatchClock): boolean {
  return clock.maxBankMs >= MAX_CLOCK_MS;
}

/** True when two clocks name the same terms. */
export function matchClockEquals(a: MatchClock, b: MatchClock): boolean {
  return (
    a.initialMs === b.initialMs && a.incrementMs === b.incrementMs && a.maxBankMs === b.maxBankMs
  );
}

/** The preset a clock is, or null for terms a host wrote themselves. */
export function findClockPreset(clock: MatchClock): ClockPreset | null {
  return CLOCK_PRESETS.find((preset) => matchClockEquals(preset.clock, clock)) ?? null;
}

/**
 * How hard a bank is pressing on the seat that holds it.
 *
 * The thresholds are a share of the bank the match opened on rather than a
 * fixed span, because the same number of minutes is the whole match on a live
 * clock and a rounding error on an async one. The opening bank is what the
 * measure is taken against and not the ceiling: a clock may be left uncapped,
 * and a share of a limit nobody reaches measures nothing. Each share has a
 * floor, so a very short clock still warns in time to act on.
 */
export type ClockPressure = "steady" | "low" | "critical";

export function clockPressure(remaining: number, initialMs: number): ClockPressure {
  if (remaining <= Math.max(initialMs * 0.1, 30_000)) return "critical";
  if (remaining <= Math.max(initialMs * 0.25, 2 * MINUTE_MS)) return "low";
  return "steady";
}

/** What a seat has left, whether or not its turn is the open one. */
export function seatRemainingMs(
  state: Pick<MatchClockState, "activeSlot" | "banksMs" | "deadlineAt">,
  slotIndex: number,
  now: number,
): number {
  return slotIndex === state.activeSlot
    ? Math.max(0, state.deadlineAt - now)
    : Math.max(0, state.banksMs[slotIndex] ?? 0);
}

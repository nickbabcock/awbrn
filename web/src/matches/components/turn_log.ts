import type { UnitKind } from "#/wasm/awbrn_wasm.js";

/**
 * Reading a game's outline as a log.
 *
 * The outline holds one boundary for each moment the game can be read at, the
 * opening included, so boundary `k` is the game after `k` actions and action
 * `k` is the one that reached it. A log line is an action: the day it was
 * taken on, its number within that day, and who took it.
 */
export interface LogBoundary {
  /** The day the game is on at this boundary. */
  day: number;
  /** The seat whose action reached this boundary; absent at the opening. */
  actingSlot: number | null;
  /** What the action was, where the outline can say. */
  summary?: ActionSummary | null;
}

/**
 * One action in a line: the unit, the verb and the tile it happened on. An
 * action that happens nowhere on the board (ending a turn, a power) has no
 * tile.
 */
export interface ActionSummary {
  unit: UnitKind | null;
  verb: string;
  tile: { x: number; y: number } | null;
}

export interface LogLine {
  /** The boundary the action reached, which is where reading it lands. */
  index: number;
  /** Its number within its day, from 1. */
  number: number;
  day: number;
  actingSlot: number | null;
  summary: ActionSummary | null;
}

/**
 * The day action `index` was taken on. The opening has no action, so it reads
 * as its own day.
 */
export function actionDay(boundaries: readonly LogBoundary[], index: number): number | null {
  if (boundaries.length === 0) return null;
  const clamped = Math.min(Math.max(index, 0), boundaries.length - 1);
  // An action that ends the last turn of a day lands on the next day, but it
  // was taken on the day before it.
  const before = boundaries[clamped === 0 ? 0 : clamped - 1];
  return before?.day ?? null;
}

/** Every action taken on one day, in order. */
export function dayLines(boundaries: readonly LogBoundary[], day: number): LogLine[] {
  const lines: LogLine[] = [];
  for (let index = 1; index < boundaries.length; index += 1) {
    if (boundaries[index - 1]?.day !== day) continue;
    const boundary = boundaries[index]!;
    lines.push({
      index,
      number: lines.length + 1,
      day,
      actingSlot: boundary.actingSlot,
      summary: boundary.summary ?? null,
    });
  }
  return lines;
}

/** The days the outline holds actions on, first to last. */
export function logDays(boundaries: readonly LogBoundary[]): number[] {
  const days: number[] = [];
  for (let index = 1; index < boundaries.length; index += 1) {
    const day = boundaries[index - 1]!.day;
    if (days[days.length - 1] !== day) days.push(day);
  }
  return days;
}

/** Where each day begins, as boundary indexes: the scrubber's ticks. */
export function dayStarts(boundaries: readonly { day: number }[]): number[] {
  const starts: number[] = [];
  for (let index = 1; index < boundaries.length; index += 1) {
    if (boundaries[index]?.day !== boundaries[index - 1]?.day) starts.push(index);
  }
  return starts;
}

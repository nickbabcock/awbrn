import type { ReviewBoundary } from "./match_protocol";

/**
 * Reading a match's outline.
 *
 * The outline is one entry for each boundary the match can be read at, the
 * opening included, so entry `k` describes the match after `k` actions. It
 * carries no board: what it is for is naming a moment and finding where the
 * turns beside it begin, which is what turn-by-turn stepping is spelled
 * against.
 *
 * A turn is a day and the seat holding it. Both come from the engine, which
 * decided them by the rules; nothing here works out whose turn it is.
 */

function turnAt(boundaries: readonly ReviewBoundary[], index: number): string | null {
  const boundary = boundaries[index];
  if (boundary === undefined || boundary.activeSlot === null) return null;
  return `${boundary.day}:${boundary.activeSlot}`;
}

/** Where the turn holding this boundary began. */
export function turnStart(boundaries: readonly ReviewBoundary[], index: number): number {
  const turn = turnAt(boundaries, index);
  if (turn === null) return index;

  let start = index;
  while (start > 0 && turnAt(boundaries, start - 1) === turn) {
    start -= 1;
  }
  return start;
}

/** Where the turn after this boundary's turn begins, or null in the last one. */
export function nextTurnStart(boundaries: readonly ReviewBoundary[], index: number): number | null {
  const turn = turnAt(boundaries, index);
  if (turn === null) return null;

  for (let candidate = index + 1; candidate < boundaries.length; candidate += 1) {
    const next = turnAt(boundaries, candidate);
    // A boundary past the end of the match holds no turn, so it is not one to
    // step on to: what is after the last turn is nothing.
    if (next === null) return null;
    if (next !== turn) return candidate;
  }
  return null;
}

/**
 * Where the turn before this boundary's turn began, or null in the first one.
 *
 * A viewer part-way through a turn is taken to the start of the turn they are
 * reading first, which is the step back they meant.
 */
export function previousTurnStart(
  boundaries: readonly ReviewBoundary[],
  index: number,
): number | null {
  const start = turnStart(boundaries, index);
  if (start < index) return start;
  return start > 0 ? turnStart(boundaries, start - 1) : null;
}

/**
 * Where a step of one size in one direction lands, or null for a step there is
 * nowhere to take.
 *
 * An action step is a count and a turn step is a search, but a caller asking
 * for either is asking the same question — where does this key put me — so
 * both are answered here rather than at each key.
 */
export function stepTarget(
  boundaries: readonly ReviewBoundary[],
  from: number,
  kind: "action" | "turn",
  delta: number,
): number | null {
  const target =
    kind === "action"
      ? Math.min(Math.max(from + delta, 0), boundaries.length - 1)
      : delta < 0
        ? previousTurnStart(boundaries, from)
        : nextTurnStart(boundaries, from);
  return target === null || target === from ? null : target;
}

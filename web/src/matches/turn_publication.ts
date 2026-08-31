import type { MatchClockState } from "./match_clock.ts";

/**
 * The turn a match reports to the global database.
 *
 * Whose turn it is lives in the match durable object, which reads it back from
 * its event log. A screen that asks "which of my matches are waiting for me"
 * asks it of every match at once, so the durable object publishes this much of
 * its state and the question becomes one query.
 */
export interface PublishedTurn {
  /** The seat on the move, or null when no turn is open. */
  activeSlotIndex: number | null;
  /** When that seat runs out, in milliseconds, or null with no open turn. */
  turnDeadlineAt: number | null;
}

/** What a match with no turn open reports: not started yet, or finished. */
export const NO_OPEN_TURN: PublishedTurn = { activeSlotIndex: null, turnDeadlineAt: null };

/** The turn a running match is on, or `NO_OPEN_TURN` for one that has ended. */
export function turnFromClock(clock: MatchClockState | null, isFinished: boolean): PublishedTurn {
  if (isFinished || clock === null) {
    return NO_OPEN_TURN;
  }
  return { activeSlotIndex: clock.activeSlot, turnDeadlineAt: clock.deadlineAt };
}

/** True when two reports name the same turn. */
export function publishedTurnEquals(a: PublishedTurn, b: PublishedTurn): boolean {
  return a.activeSlotIndex === b.activeSlotIndex && a.turnDeadlineAt === b.turnDeadlineAt;
}

/**
 * What to write, or null when the last report still stands.
 *
 * A turn is played over many actions and every one of them reaches the durable
 * object, so the write is decided here rather than made for each: comparing the
 * turn against the last one published is what keeps the cost at one write for
 * each turn instead of one for each action.
 */
export function turnPublicationUpdate(
  turn: PublishedTurn,
  published: PublishedTurn | null | undefined,
): PublishedTurn | null {
  if (published && publishedTurnEquals(turn, published)) {
    return null;
  }
  return turn;
}

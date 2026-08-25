import { z } from "zod";
import { decodeCursor } from "./cursor";
import { matchIdSchema } from "./match_id";
import type { MatchOutcome, MatchPhase, MatchHistorySeat, SeatResultReason } from "./schemas";

/** Finished matches are the only ones this record holds. */
export const COMPLETED_MATCH_PHASE: MatchPhase = "completed";

export const MATCH_HISTORY_PAGE_SIZE = 30;

const matchHistoryCursorSchema = z.object({
  completedAt: z.iso.datetime(),
  matchId: matchIdSchema,
});

export type MatchHistoryCursor = z.infer<typeof matchHistoryCursorSchema>;

export function encodeMatchHistoryCursor(cursor: MatchHistoryCursor): string {
  return JSON.stringify(cursor);
}

export function decodeMatchHistoryCursor(cursor: string | undefined): MatchHistoryCursor | null {
  return decodeCursor(cursor, matchHistoryCursorSchema);
}

/** Best outcome first, so a viewer who held several seats reads their best. */
const outcomeRank: Record<MatchOutcome, number> = {
  win: 0,
  draw: 1,
  loss: 2,
};

/**
 * The outcome to report for a viewer who held one or more seats.
 *
 * Null when no seat has a recorded result, which happens while a match ended
 * but its results were not written.
 */
export function viewerOutcome(seats: readonly MatchHistorySeat[]): MatchOutcome | null {
  let best: MatchOutcome | null = null;
  for (const seat of seats) {
    if (seat.outcome === null) continue;
    if (best === null || outcomeRank[seat.outcome] < outcomeRank[best]) {
      best = seat.outcome;
    }
  }
  return best;
}

/** The verdict, in the HUD voice the readouts use. */
export function formatVerdict(outcome: MatchOutcome | null): string {
  switch (outcome) {
    case "win":
      return "Victory";
    case "loss":
      return "Defeat";
    case "draw":
      return "Draw";
    case null:
      return "No result";
  }
}

/**
 * Why the seat ended where it did, in the game's own words.
 *
 * A win with no reason is the army that was still standing at the end.
 */
export function formatSeatResultReason(
  outcome: MatchOutcome | null,
  reason: SeatResultReason | null,
): string {
  if (reason === null) {
    return outcome === "win" ? "Last army standing" : "Result not recorded";
  }

  switch (reason) {
    case "rout":
      return "Army destroyed";
    case "hq-capture":
      return "HQ captured";
    case "lab-capture":
      return "Lab captured";
    case "capture-limit":
      return "Capture limit";
    case "day-limit":
      return "Day limit";
    case "resignation":
      return "Resigned";
    case "timeout":
      return "Timed out";
    case "agreement":
      return "Agreed draw";
  }
}

/**
 * How long the match ran, from the first day to the last.
 *
 * Null while a match has no start time, which no finished match written by the
 * current server lacks, but older rows can.
 */
export function formatMatchDuration(startedAt: string | null, completedAt: string): string | null {
  if (startedAt === null) {
    return null;
  }

  const started = Date.parse(startedAt);
  const completed = Date.parse(completedAt);
  if (!Number.isFinite(started) || !Number.isFinite(completed) || completed < started) {
    return null;
  }

  const minutes = Math.round((completed - started) / 60_000);
  if (minutes < 60) {
    return `${Math.max(1, minutes)} min`;
  }

  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return `${hours} hr`;
  }

  return `${Math.round(hours / 24)} days`;
}

/** The seats the viewer did not hold, in slot order. */
export function opposingSeats(
  seats: readonly MatchHistorySeat[],
  viewerSlotIndexes: readonly number[],
): MatchHistorySeat[] {
  const held = new Set(viewerSlotIndexes);
  return seats.filter((seat) => !held.has(seat.slotIndex));
}

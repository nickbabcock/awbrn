import { z } from "zod";
import { decodeCursor } from "#/utils/cursor.ts";
import { matchIdSchema } from "./match_id";
import type {
  MatchOutcome,
  MatchPhase,
  MatchHistorySeat,
  RankedPool,
  SeatResultReason,
} from "./schemas";

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
 * What the match did to the viewer's rating, in one line.
 *
 * An unranked match has no line at all. A ranked match which the pool's rating
 * writer has not reached says so rather than showing nothing, because the
 * report can be open before the rating moves.
 */
export function formatRatingChange(
  seat: { ratingBefore: number | null; ratingAfter: number | null } | undefined,
  pool: RankedPool | null,
): string | null {
  if (pool === null) return null;
  if (!seat || seat.ratingBefore === null || seat.ratingAfter === null) {
    return "Rating pending";
  }

  const before = Math.round(seat.ratingBefore);
  const after = Math.round(seat.ratingAfter);
  const change = after - before;
  const sign = change >= 0 ? "+" : "-";
  return `${before} → ${after} (${sign}${Math.abs(change)})`;
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

/** The seats the reported player did not hold, in slot order. */
export function opposingSeats(
  seats: readonly MatchHistorySeat[],
  playerSlotIndexes: readonly number[],
): MatchHistorySeat[] {
  const held = new Set(playerSlotIndexes);
  return seats.filter((seat) => !held.has(seat.slotIndex));
}

/**
 * The reason every seat shares (an agreed draw, the day limit), which a report
 * says once rather than on each seat. Null when the seats ended differently.
 */
export function sharedSeatReason(seats: readonly MatchHistorySeat[]): SeatResultReason | null {
  const first = seats[0]?.reason ?? null;
  if (first === null || seats.length < 2) return null;
  return seats.every((seat) => seat.reason === first) ? first : null;
}

/**
 * What a report says on one seat. A seat with no recorded reason says
 * nothing, except a hotseat's winner: its tag says "Hotseat", so the seat
 * says who won. A reason every seat shares is said once, not on each seat.
 */
export function seatReason(
  seat: MatchHistorySeat,
  { isHotseat, shared }: { isHotseat: boolean; shared: SeatResultReason | null },
): string | null {
  if (seat.reason !== null) {
    return shared === null ? formatSeatResultReason(seat.outcome, seat.reason) : null;
  }
  return isHotseat && seat.outcome === "win" ? "Won" : null;
}

/**
 * A report's result tag. A loss among more than two armies is the place the
 * player finished (`2nd`); otherwise the outcome, said to the viewer on their
 * own report (`You won`) and about the player on anyone else's (`Won`).
 */
export function formatReportResult({
  isHotseat,
  isViewer,
  outcome,
  placement,
  seatCount,
}: {
  isHotseat: boolean;
  isViewer: boolean;
  outcome: MatchOutcome | null;
  placement: number | null;
  seatCount: number;
}): { label: string; tone: "win" | "plain" | "place" } {
  if (isHotseat) return { label: "Hotseat", tone: "plain" };
  if (outcome === "loss" && seatCount > 2 && placement !== null && placement > 1) {
    return { label: ordinal(placement), tone: "place" };
  }
  switch (outcome) {
    case "win":
      return { label: isViewer ? "You won" : "Won", tone: "win" };
    case "loss":
      return { label: isViewer ? "You lost" : "Lost", tone: "plain" };
    case "draw":
      return { label: isViewer ? "A draw" : "Draw", tone: "plain" };
    case null:
      return { label: "No result", tone: "plain" };
  }
}

/** `1st`, `2nd`, `11th`, `22nd`. */
export function ordinal(place: number): string {
  const tens = place % 100;
  if (tens >= 11 && tens <= 13) return `${place}th`;
  switch (place % 10) {
    case 1:
      return `${place}st`;
    case 2:
      return `${place}nd`;
    case 3:
      return `${place}rd`;
    default:
      return `${place}th`;
  }
}

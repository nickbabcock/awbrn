import type { MatchResults } from "#/wasm/awbrn_server.js";
import type {
  AiProfileId,
  MatchOutcome,
  MatchSetup,
  RankedPool,
  SeatResultReason,
} from "./schemas.ts";

/** One `match_results` row for the global database. */
export interface MatchResultRow {
  matchId: string;
  slotIndex: number;
  /** The person who held the seat, or null when the server played it. */
  userId: string | null;
  aiProfileId: AiProfileId | null;
  teamId: string | null;
  outcome: MatchOutcome;
  placement: number;
  reason: SeatResultReason | null;
  pool: RankedPool | null;
}

/**
 * Build result rows. Seats without a matching setup player are skipped.
 *
 * Every seat is recorded, including one the server played: the match happened,
 * and a report that showed only half of it would be a worse record than none.
 * What such a match never carries is a pool. A rating is between people, so a
 * match with a seat nobody held is unranked whatever it was opened as.
 */
export function matchResultRows(setup: MatchSetup, results: MatchResults): MatchResultRow[] {
  const serverPlayed = setup.players.some((player) => (player.aiProfileId ?? null) !== null);
  const pool = serverPlayed ? null : (setup.pool ?? null);

  return results.seats.flatMap((seat) => {
    const player = setup.players[seat.slotIndex];
    if (!player) return [];
    return [
      {
        matchId: setup.matchId,
        slotIndex: seat.slotIndex,
        userId: player.userId ?? null,
        aiProfileId: player.aiProfileId ?? null,
        teamId: seat.teamId,
        outcome: seat.outcome,
        placement: seat.placement,
        reason: seat.reason ?? null,
        pool,
      },
    ];
  });
}

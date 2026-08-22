import type { MatchResults } from "#/wasm/awbrn_server.js";
import type { MatchOutcome, MatchSetup, RankedPool, SeatResultReason } from "./schemas.ts";

/** One `match_results` row for the global database. */
export interface MatchResultRow {
  matchId: string;
  slotIndex: number;
  userId: string;
  teamId: string | null;
  outcome: MatchOutcome;
  placement: number;
  reason: SeatResultReason | null;
  pool: RankedPool | null;
}

/** Build result rows. Seats without a matching user are skipped. */
export function matchResultRows(setup: MatchSetup, results: MatchResults): MatchResultRow[] {
  return results.seats.flatMap((seat) => {
    const player = setup.players[seat.slotIndex];
    if (!player) return [];
    return [
      {
        matchId: setup.matchId,
        slotIndex: seat.slotIndex,
        userId: player.userId,
        teamId: seat.teamId,
        outcome: seat.outcome,
        placement: seat.placement,
        reason: seat.reason ?? null,
        pool: null,
      },
    ];
  });
}

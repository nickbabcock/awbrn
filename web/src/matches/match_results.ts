import type { DrawReason, PlayerStatus, VictoryReason } from "#/wasm/awbrn_server.js";
import type { MatchOutcome, MatchSeatStatus, RankedPool, SeatResultReason } from "./schemas.ts";

/** Keep result reasons and statuses in sync with the engine vocabulary. */
type AssertNever<T extends never> = T;
export type EngineReasonsAreCovered = AssertNever<
  Exclude<VictoryReason | DrawReason, SeatResultReason>
>;
export type SeatReasonsAreEngineReasons = AssertNever<
  Exclude<SeatResultReason, VictoryReason | DrawReason>
>;
export type EngineStatusesAreCovered = AssertNever<Exclude<PlayerStatus, MatchSeatStatus>>;
export type SeatStatusesAreEngineStatuses = AssertNever<Exclude<MatchSeatStatus, PlayerStatus>>;

/** First place, held by every winning seat and by every seat of a draw. */
export const PLACEMENT_FIRST = 1;

/** Map a result reason to its terminal seat status. */
export function seatStatus(reason: SeatResultReason | null): MatchSeatStatus {
  switch (reason) {
    case null:
      return "active";
    case "resignation":
      return "resigned";
    case "timeout":
      return "timed-out";
    case "rout":
    case "hq-capture":
    case "lab-capture":
      return "eliminated";
    case "capture-limit":
    case "day-limit":
    case "agreement":
      return "active";
  }
}

export interface MatchResultOutcome {
  outcome: MatchOutcome;
  pool: RankedPool | null;
  /** True when a `match_voids` row exists for this seat's match. */
  voided: boolean;
}

/** Whether a pooled, non-voided result counts for ratings. */
export function isRatedResult(result: MatchResultOutcome): boolean {
  return result.pool !== null && !result.voided;
}

/** The Glicko-2 score for a seat: 1 for a win, 0.5 for a draw, 0 for a loss. */
export function glickoScore(outcome: MatchOutcome): number {
  switch (outcome) {
    case "win":
      return 1;
    case "draw":
      return 0.5;
    case "loss":
      return 0;
  }
}

/** Whether a winning seat left before its team won. */
export function wonAfterLeaving(result: {
  outcome: MatchOutcome;
  reason: SeatResultReason | null;
}): boolean {
  return result.outcome === "win" && seatStatus(result.reason) !== "active";
}

/** Whether a positive integer placement agrees with the outcome. */
export function placementMatchesOutcome(outcome: MatchOutcome, placement: number): boolean {
  if (!Number.isInteger(placement) || placement < PLACEMENT_FIRST) return false;
  return (placement === PLACEMENT_FIRST) === (outcome === "win" || outcome === "draw");
}

/** Whether the result reason is valid for the seat outcome. */
export function reasonMatchesOutcome(
  outcome: MatchOutcome,
  reason: SeatResultReason | null,
): boolean {
  return reason !== null || outcome === "win";
}

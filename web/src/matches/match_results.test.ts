import { describe, expect, it } from "vitest";
import {
  isRatedResult,
  placementMatchesOutcome,
  reasonMatchesOutcome,
  glickoScore,
  seatStatus,
  wonAfterLeaving,
} from "./match_results.ts";
import {
  matchOutcomeSchema,
  rankedPoolSchema,
  seatResultReasonSchema,
  type SeatResultReason,
} from "./schemas.ts";

const decisiveReasons = [
  "rout",
  "hq-capture",
  "lab-capture",
  "capture-limit",
  "day-limit",
  "resignation",
  "timeout",
] as const satisfies readonly SeatResultReason[];

describe("match results", () => {
  it("rates every real result in a pool, draws included", () => {
    expect(decisiveReasons.length).toBe(7);
    for (const outcome of ["win", "loss", "draw"] as const) {
      expect(isRatedResult({ outcome, pool: "fog_async", voided: false })).toBe(true);
    }
  });

  it("scores a draw at a half point for Glicko-2", () => {
    expect(glickoScore("win")).toBe(1);
    expect(glickoScore("draw")).toBe(0.5);
    expect(glickoScore("loss")).toBe(0);
  });

  it("stops a voided match counting without rewriting what happened", () => {
    const win = { outcome: "win", pool: "async" } as const;
    expect(isRatedResult({ ...win, voided: false })).toBe(true);
    expect(isRatedResult({ ...win, voided: true })).toBe(false);
    // Voiding does not change the stored outcome.
    expect(glickoScore(win.outcome)).toBe(1);
  });

  it("leaves poolless matches unrated", () => {
    expect(isRatedResult({ outcome: "win", pool: null, voided: false })).toBe(false);
  });

  it("derives a seat's status from its reason", () => {
    expect(seatStatus(null)).toBe("active");
    expect(seatStatus("resignation")).toBe("resigned");
    expect(seatStatus("timeout")).toBe("timed-out");
    expect(seatStatus("rout")).toBe("eliminated");
    expect(seatStatus("hq-capture")).toBe("eliminated");
    expect(seatStatus("lab-capture")).toBe("eliminated");
    // These endings leave the seat active.
    expect(seatStatus("capture-limit")).toBe("active");
    expect(seatStatus("day-limit")).toBe("active");
    expect(seatStatus("agreement")).toBe("active");
  });

  it("leaves the loser of a capture limit standing but names why it lost", () => {
    // The limit ends the match without eliminating the loser.
    const loser = { outcome: "loss", reason: "capture-limit", placement: 2 } as const;
    expect(seatStatus(loser.reason)).toBe("active");
    expect(isRatedResult({ ...loser, pool: "async", voided: false })).toBe(true);
  });

  it("recovers a 1v1 ending from the loser's reason alone", () => {
    // The non-null reason identifies the 1v1 ending.
    const endings = [
      { end: "rout", seats: [null, "rout"] },
      { end: "hq-capture", seats: [null, "hq-capture"] },
      { end: "capture-limit", seats: [null, "capture-limit"] },
      { end: "day-limit", seats: [null, "day-limit"] },
      { end: "agreement", seats: ["agreement", "agreement"] },
    ] as const;
    for (const { end, seats } of endings) {
      expect(seats.find((reason) => reason !== null)).toBe(end);
    }
  });

  it("wins a team match for a member eliminated before their allies finished", () => {
    // Red wins after red-2 is eliminated.
    const redTwo = { outcome: "win", reason: "rout", placement: 1 } as const;
    const redOne = { outcome: "win", reason: null, placement: 1 } as const;

    expect(wonAfterLeaving(redTwo)).toBe(true);
    expect(seatStatus(redTwo.reason)).toBe("eliminated");
    expect(wonAfterLeaving(redOne)).toBe(false);
    expect(seatStatus(redOne.reason)).toBe("active");
    for (const seat of [redOne, redTwo]) {
      expect(placementMatchesOutcome(seat.outcome, seat.placement)).toBe(true);
    }
  });

  it("places wins and draws first, and ranks everyone else", () => {
    expect(placementMatchesOutcome("win", 1)).toBe(true);
    expect(placementMatchesOutcome("draw", 1)).toBe(true);
    expect(placementMatchesOutcome("loss", 2)).toBe(true);
    expect(placementMatchesOutcome("loss", 4)).toBe(true);
    expect(placementMatchesOutcome("win", 2)).toBe(false);
    expect(placementMatchesOutcome("loss", 1)).toBe(false);
    expect(placementMatchesOutcome("draw", 2)).toBe(false);
  });

  it("rejects a placement below first, as the check constraint does", () => {
    expect(placementMatchesOutcome("loss", 0)).toBe(false);
    expect(placementMatchesOutcome("loss", -3)).toBe(false);
    expect(placementMatchesOutcome("win", 0)).toBe(false);
  });

  it("rejects a placement that is not a whole rank", () => {
    // Reject fractional and non-finite ranks.
    for (const outcome of ["win", "loss", "draw"] as const) {
      for (const placement of [1.5, 2.5, NaN, Infinity, -Infinity]) {
        expect(placementMatchesOutcome(outcome, placement)).toBe(false);
      }
    }
    // Whole-number ranks pass.
    expect(placementMatchesOutcome("loss", 2.0)).toBe(true);
  });

  it("lets only a standing winner leave its reason null", () => {
    expect(reasonMatchesOutcome("win", null)).toBe(true);
    expect(reasonMatchesOutcome("loss", null)).toBe(false);
    // A draw must record the ending.
    expect(reasonMatchesOutcome("draw", null)).toBe(false);
    expect(reasonMatchesOutcome("draw", "day-limit")).toBe(true);
    expect(reasonMatchesOutcome("draw", "agreement")).toBe(true);
    // An early winner carries its cause.
    expect(reasonMatchesOutcome("win", "rout")).toBe(true);
    for (const reason of decisiveReasons) {
      expect(reasonMatchesOutcome("loss", reason)).toBe(true);
    }
  });

  it("records a free-for-all where seats left for different reasons", () => {
    const ffa = [
      { outcome: "win", reason: null, placement: 1 },
      { outcome: "loss", reason: "rout", placement: 2 },
      { outcome: "loss", reason: "hq-capture", placement: 3 },
      { outcome: "loss", reason: "resignation", placement: 4 },
    ] as const;
    expect(ffa.map((seat) => seatStatus(seat.reason))).toEqual([
      "active",
      "eliminated",
      "eliminated",
      "resigned",
    ]);
    for (const seat of ffa) {
      expect(placementMatchesOutcome(seat.outcome, seat.placement)).toBe(true);
      expect(reasonMatchesOutcome(seat.outcome, seat.reason)).toBe(true);
    }
  });

  it("speaks the engine's vocabulary", () => {
    expect(seatResultReasonSchema.safeParse("lab-capture").success).toBe(true);
    expect(seatResultReasonSchema.safeParse("hq_capture").success).toBe(false);
    expect(seatResultReasonSchema.safeParse("surrender").success).toBe(false);
    expect(matchOutcomeSchema.safeParse("draw").success).toBe(true);
    expect(rankedPoolSchema.safeParse("fog_live").success).toBe(true);
    expect(rankedPoolSchema.safeParse("blitz").success).toBe(false);
  });

  it("keeps void out of the result vocabulary entirely", () => {
    // Voiding is separate from the result.
    expect(seatResultReasonSchema.safeParse("void").success).toBe(false);
    expect(matchOutcomeSchema.safeParse("void").success).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import {
  decodeMatchHistoryCursor,
  encodeMatchHistoryCursor,
  formatMatchDuration,
  formatSeatResultReason,
  formatVerdict,
  opposingSeats,
  viewerOutcome,
} from "./match_history";
import type { MatchHistorySeat } from "./schemas";

function seat(overrides: Partial<MatchHistorySeat> = {}): MatchHistorySeat {
  return {
    slotIndex: 0,
    userId: "user-1",
    userName: "Andy",
    factionId: 1,
    coId: null,
    outcome: null,
    placement: null,
    reason: null,
    ...overrides,
  };
}

describe("viewerOutcome", () => {
  it("reports the best outcome across the seats the viewer held", () => {
    const outcome = viewerOutcome([
      seat({ slotIndex: 0, outcome: "loss", reason: "rout" }),
      seat({ slotIndex: 1, outcome: "win" }),
    ]);
    expect(outcome).toBe("win");
  });

  it("prefers a draw to a loss", () => {
    const outcome = viewerOutcome([
      seat({ slotIndex: 0, outcome: "loss", reason: "resignation" }),
      seat({ slotIndex: 1, outcome: "draw", reason: "day-limit" }),
    ]);
    expect(outcome).toBe("draw");
  });

  it("is null when no seat has a recorded result", () => {
    expect(viewerOutcome([seat(), seat({ slotIndex: 1 })])).toBeNull();
  });
});

describe("match history cursors", () => {
  it("round-trips the completion time and match id", () => {
    const cursor = { completedAt: "2026-01-02T03:04:05.000Z", matchId: "abc123def4567" };
    expect(decodeMatchHistoryCursor(encodeMatchHistoryCursor(cursor))).toEqual(cursor);
  });

  it("rejects missing and malformed cursors", () => {
    expect(decodeMatchHistoryCursor(undefined)).toBeNull();
    expect(decodeMatchHistoryCursor("not json")).toBeNull();
    expect(decodeMatchHistoryCursor('{"completedAt":"","matchId":"abc123def4567"}')).toBeNull();
  });
});

describe("formatVerdict", () => {
  it("names each outcome", () => {
    expect(formatVerdict("win")).toBe("Victory");
    expect(formatVerdict("loss")).toBe("Defeat");
    expect(formatVerdict("draw")).toBe("Draw");
    expect(formatVerdict(null)).toBe("No result");
  });
});

describe("formatSeatResultReason", () => {
  it("reads a win with no reason as the surviving army", () => {
    expect(formatSeatResultReason("win", null)).toBe("Last army standing");
  });

  it("reads a missing reason on any other outcome as an unrecorded result", () => {
    expect(formatSeatResultReason("loss", null)).toBe("Result not recorded");
    expect(formatSeatResultReason(null, null)).toBe("Result not recorded");
  });

  it("names every reason the engine can record", () => {
    expect(formatSeatResultReason("loss", "rout")).toBe("Army destroyed");
    expect(formatSeatResultReason("loss", "hq-capture")).toBe("HQ captured");
    expect(formatSeatResultReason("loss", "lab-capture")).toBe("Lab captured");
    expect(formatSeatResultReason("win", "capture-limit")).toBe("Capture limit");
    expect(formatSeatResultReason("draw", "day-limit")).toBe("Day limit");
    expect(formatSeatResultReason("loss", "resignation")).toBe("Resigned");
    expect(formatSeatResultReason("loss", "timeout")).toBe("Timed out");
    expect(formatSeatResultReason("draw", "agreement")).toBe("Agreed draw");
  });
});

describe("formatMatchDuration", () => {
  it("reports minutes below an hour", () => {
    expect(formatMatchDuration("2026-01-01T00:00:00.000Z", "2026-01-01T00:42:00.000Z")).toBe(
      "42 min",
    );
  });

  it("never reports a finished match as zero minutes", () => {
    expect(formatMatchDuration("2026-01-01T00:00:00.000Z", "2026-01-01T00:00:10.000Z")).toBe(
      "1 min",
    );
  });

  it("reports hours up to two days, then days", () => {
    expect(formatMatchDuration("2026-01-01T00:00:00.000Z", "2026-01-01T06:00:00.000Z")).toBe(
      "6 hr",
    );
    expect(formatMatchDuration("2026-01-01T00:00:00.000Z", "2026-01-05T00:00:00.000Z")).toBe(
      "4 days",
    );
  });

  it("is null when the match has no start time or the times disagree", () => {
    expect(formatMatchDuration(null, "2026-01-01T00:00:00.000Z")).toBeNull();
    expect(formatMatchDuration("2026-01-02T00:00:00.000Z", "2026-01-01T00:00:00.000Z")).toBeNull();
  });
});

describe("opposingSeats", () => {
  it("keeps every seat the viewer did not hold, in order", () => {
    const seats = [
      seat({ slotIndex: 0 }),
      seat({ slotIndex: 1, userId: "user-2", userName: "Max" }),
      seat({ slotIndex: 2, userId: "user-3", userName: "Sami" }),
    ];
    expect(opposingSeats(seats, [0]).map((entry) => entry.userName)).toEqual(["Max", "Sami"]);
  });

  it("is empty when the viewer held every seat", () => {
    const seats = [seat({ slotIndex: 0 }), seat({ slotIndex: 1 })];
    expect(opposingSeats(seats, [0, 1])).toEqual([]);
  });
});

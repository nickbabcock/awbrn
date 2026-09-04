import { describe, expect, it } from "vitest";
import { nextTurnStart, previousTurnStart, stepTarget, turnStart } from "./match_review";
import type { ReviewBoundary } from "./match_protocol";

/** Two seats, three actions each, over two days. */
function outline(): ReviewBoundary[] {
  const turns: [number, number][] = [
    [1, 0],
    [1, 1],
    [2, 0],
    [2, 1],
  ];
  return turns.flatMap(([day, activeSlot]) =>
    Array.from({ length: 3 }, () => ({ day, actingSlot: activeSlot, activeSlot })),
  );
}

describe("reading a match outline", () => {
  it("finds the boundary a turn began at", () => {
    expect(turnStart(outline(), 5)).toBe(3);
    expect(turnStart(outline(), 3)).toBe(3);
    expect(turnStart(outline(), 0)).toBe(0);
  });

  it("steps on to the first boundary of the next turn", () => {
    expect(nextTurnStart(outline(), 0)).toBe(3);
    expect(nextTurnStart(outline(), 4)).toBe(6);
  });

  it("has nothing after the last turn", () => {
    expect(nextTurnStart(outline(), 11)).toBeNull();
  });

  it("steps back from the middle of a turn to that turn's start", () => {
    expect(previousTurnStart(outline(), 4)).toBe(3);
  });

  it("steps back from the start of a turn to the turn before it", () => {
    expect(previousTurnStart(outline(), 3)).toBe(0);
  });

  it("has nothing before the first turn", () => {
    expect(previousTurnStart(outline(), 0)).toBeNull();
  });

  it("tells two turns of the same seat on different days apart", () => {
    const boundaries: ReviewBoundary[] = [
      { day: 1, actingSlot: null, activeSlot: 0 },
      { day: 2, actingSlot: 0, activeSlot: 0 },
    ];
    expect(nextTurnStart(boundaries, 0)).toBe(1);
  });

  it("does not step on past the last turn of a finished match", () => {
    const boundaries: ReviewBoundary[] = [
      { day: 1, actingSlot: null, activeSlot: 0 },
      { day: 1, actingSlot: 0, activeSlot: 0 },
      { day: 1, actingSlot: 0, activeSlot: null },
    ];
    expect(nextTurnStart(boundaries, 0)).toBeNull();
    expect(nextTurnStart(boundaries, 2)).toBeNull();
    expect(previousTurnStart(boundaries, 2)).toBe(0);
    expect(stepTarget(boundaries, 0, "turn", 1)).toBeNull();
  });

  it("reads a match nobody has played yet", () => {
    const boundaries: ReviewBoundary[] = [{ day: 1, actingSlot: null, activeSlot: 0 }];
    expect(nextTurnStart(boundaries, 0)).toBeNull();
    expect(previousTurnStart(boundaries, 0)).toBeNull();
  });
});

describe("stepping through a match", () => {
  it("counts actions", () => {
    expect(stepTarget(outline(), 5, "action", 1)).toBe(6);
    expect(stepTarget(outline(), 5, "action", -1)).toBe(4);
  });

  it("stops at either end rather than running off it", () => {
    expect(stepTarget(outline(), 0, "action", -1)).toBeNull();
    expect(stepTarget(outline(), 11, "action", 1)).toBeNull();
  });

  it("crosses turns", () => {
    expect(stepTarget(outline(), 4, "turn", 1)).toBe(6);
    expect(stepTarget(outline(), 4, "turn", -1)).toBe(3);
  });

  it("has no turn to cross to at either end", () => {
    expect(stepTarget(outline(), 0, "turn", -1)).toBeNull();
    expect(stepTarget(outline(), 11, "turn", 1)).toBeNull();
  });
});

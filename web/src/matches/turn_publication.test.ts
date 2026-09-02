import { describe, expect, it } from "vitest";
import {
  NO_OPEN_TURN,
  turnFromClock,
  turnPublicationUpdate,
  type PublishedTurn,
} from "./turn_publication.ts";
import type { MatchClockState } from "./match_clock.ts";

const CLOCK: MatchClockState = {
  banksMs: { 0: 1000, 1: 2000 },
  turnStartedAt: 500,
  activeSlot: 1,
  deadlineAt: 2500,
};

describe("turn publication", () => {
  it("reads the open turn from the clock", () => {
    expect(turnFromClock(CLOCK, false)).toEqual({ activeSlotIndex: 1, turnDeadlineAt: 2500 });
  });

  it("closes the turn of a finished match", () => {
    expect(turnFromClock(CLOCK, true)).toEqual(NO_OPEN_TURN);
    expect(turnFromClock(null, false)).toEqual(NO_OPEN_TURN);
  });

  it("writes nothing while the turn stands", () => {
    const published: PublishedTurn = { activeSlotIndex: 1, turnDeadlineAt: 2500 };
    expect(turnPublicationUpdate(turnFromClock(CLOCK, false), published)).toBeNull();
  });

  it("writes the turn nothing has published yet", () => {
    expect(turnPublicationUpdate(turnFromClock(CLOCK, false), undefined)).toEqual({
      activeSlotIndex: 1,
      turnDeadlineAt: 2500,
    });
  });

  it("writes a turn that moved on, and one whose deadline moved", () => {
    const published: PublishedTurn = { activeSlotIndex: 0, turnDeadlineAt: 500 };
    expect(turnPublicationUpdate(turnFromClock(CLOCK, false), published)).toEqual({
      activeSlotIndex: 1,
      turnDeadlineAt: 2500,
    });
    expect(turnPublicationUpdate({ activeSlotIndex: 0, turnDeadlineAt: 900 }, published)).toEqual({
      activeSlotIndex: 0,
      turnDeadlineAt: 900,
    });
  });

  it("clears the turn of a match that has just finished", () => {
    const published: PublishedTurn = { activeSlotIndex: 1, turnDeadlineAt: 2500 };
    expect(turnPublicationUpdate(turnFromClock(CLOCK, true), published)).toEqual(NO_OPEN_TURN);
    expect(turnPublicationUpdate(NO_OPEN_TURN, NO_OPEN_TURN)).toBeNull();
  });
});

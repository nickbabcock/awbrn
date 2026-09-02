import { describe, expect, it } from "vitest";
import {
  CLOCK_PRESETS,
  clockPressure,
  clockTickMs,
  commandEndsTurn,
  computeMatchClock,
  findClockPreset,
  formatClockCountdown,
  formatClockDuration,
  formatClockSummary,
  formatTurnRemaining,
  isBankUncapped,
  remainingMs,
  seatRemainingMs,
} from "./match_clock.ts";
import { advanceClockProgress, readClockProgress, startClockProgress } from "./match_clock.ts";
import type { ClockAction } from "./match_clock.ts";
import type { MatchClock } from "./schemas.ts";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const START = 1_700_000_000_000;

const clock: MatchClock = {
  initialMs: 7 * DAY,
  incrementMs: 2 * DAY,
  maxBankMs: 7 * DAY,
};

function endTurn(slotIndex: number, at: number): ClockAction {
  return { slotIndex, endsTurn: true, at };
}

describe("match clock", () => {
  it("gives every seat the starting bank before anyone has moved", () => {
    const state = computeMatchClock(clock, START, [], 0, 2);

    expect(state.turnStartedAt).toBe(START);
    expect(state.activeSlot).toBe(0);
    expect(state.deadlineAt).toBe(START + 7 * DAY);
    expect(state.banksMs).toEqual({ 0: 7 * DAY, 1: 7 * DAY });
  });

  it("banks a seat that has not closed a turn yet, so it never reads as out", () => {
    const state = computeMatchClock(clock, START, [endTurn(0, START + DAY)], 1, 3);

    expect(state.banksMs[2]).toBe(7 * DAY);
  });

  it("charges a seat for the whole span its turn was open", () => {
    const state = computeMatchClock(clock, START, [endTurn(0, START + 3 * DAY)], 1, 2);

    expect(state.banksMs[0]).toBe(7 * DAY - 3 * DAY + 2 * DAY);
    expect(state.turnStartedAt).toBe(START + 3 * DAY);
    expect(state.deadlineAt).toBe(START + 3 * DAY + 7 * DAY);
  });

  it("holds the bank at the ceiling for a seat that plays quickly", () => {
    const actions = [endTurn(0, START + HOUR), endTurn(1, START + 2 * HOUR)];

    const state = computeMatchClock(clock, START, actions, 0, 2);

    // The increment would carry the bank past seven days, so it stops there.
    expect(state.banksMs[0]).toBe(7 * DAY);
    expect(state.deadlineAt).toBe(START + 2 * HOUR + 7 * DAY);
  });

  it("charges only the seat whose turn was open", () => {
    const actions = [
      endTurn(0, START + 3 * DAY),
      { slotIndex: 1, endsTurn: false, at: START + 4 * DAY },
      endTurn(1, START + 5 * DAY),
    ];

    const state = computeMatchClock(clock, START, actions, 0, 2);

    expect(state.banksMs[0]).toBe(7 * DAY - 3 * DAY + 2 * DAY);
    expect(state.banksMs[1]).toBe(7 * DAY - 2 * DAY + 2 * DAY);
  });

  it("drains the bank of a seat that keeps taking the whole clock", () => {
    // Four days a turn against a two day increment loses two days a turn.
    let at = START;
    const actions: ClockAction[] = [];
    for (let turn = 0; turn < 2; turn += 1) {
      at += 4 * DAY;
      actions.push(endTurn(0, at));
      at += MINUTE;
      actions.push(endTurn(1, at));
    }

    const state = computeMatchClock(clock, START, actions, 0, 2);

    expect(state.banksMs[0]).toBe(7 * DAY - 8 * DAY + 4 * DAY);
    expect(state.deadlineAt).toBe(at + 3 * DAY);
  });

  it("leaves a seat that ran out of time with nothing, and no increment", () => {
    const state = computeMatchClock(clock, START, [endTurn(0, START + 8 * DAY)], 1, 2);

    expect(state.banksMs[0]).toBe(0);
  });

  it("charges nothing for a timestamp older than the turn it closes", () => {
    const actions = [endTurn(0, START + DAY), endTurn(1, START)];

    const state = computeMatchClock(clock, START, actions, 0, 2);

    expect(state.banksMs[1]).toBe(7 * DAY);
    // The stale close cannot pull the next turn's deadline back with it.
    expect(state.turnStartedAt).toBe(START + DAY);
    expect(state.deadlineAt).toBe(START + DAY + state.banksMs[0]!);
  });

  it("charges a seat that routs itself up to the action that did it", () => {
    // Slot zero deletes its last unit two days in, which passes play on with
    // no end turn command of its own. Slot one then plays for a day.
    const actions: ClockAction[] = [
      { slotIndex: 0, endsTurn: false, at: START + 2 * DAY },
      { slotIndex: 1, endsTurn: false, at: START + 2 * DAY + 12 * HOUR },
      endTurn(1, START + 3 * DAY),
    ];

    const state = computeMatchClock(clock, START, actions, 0, 2);

    expect(state.banksMs[0]).toBe(7 * DAY - 2 * DAY + 2 * DAY);
    // A day spent against a two day increment stops at the ceiling.
    expect(state.banksMs[1]).toBe(7 * DAY);
  });

  it("settles a seat that routed itself as the last recorded action", () => {
    const actions: ClockAction[] = [{ slotIndex: 0, endsTurn: false, at: START + 2 * DAY }];

    // The engine has already passed play to slot one.
    const state = computeMatchClock(clock, START, actions, 1, 2);

    expect(state.banksMs[0]).toBe(7 * DAY);
    expect(state.turnStartedAt).toBe(START + 2 * DAY);
    expect(state.deadlineAt).toBe(START + 2 * DAY + 7 * DAY);
  });

  it("reports the time the active seat has left", () => {
    const state = computeMatchClock(clock, START, [], 0, 2);

    expect(remainingMs(state, START + 2 * DAY)).toBe(5 * DAY);
    expect(remainingMs(state, START + 9 * DAY)).toBe(0);
  });

  it("reads the same clock action by action as it does from the whole log", () => {
    const actions = [
      endTurn(0, START + 2 * DAY),
      endTurn(1, START + 3 * DAY),
      { slotIndex: 0, endsTurn: false, at: START + 4 * DAY },
      endTurn(0, START + 5 * DAY),
      endTurn(1, START + 12 * DAY),
    ];

    const progress = startClockProgress(clock, START, 2);
    for (const action of actions) {
      advanceClockProgress(progress, action);
      // A read is taken between actions, as a live match takes one, to prove
      // it leaves nothing behind in the running total.
      readClockProgress(progress, 1);
    }

    expect(readClockProgress(progress, 0)).toEqual(computeMatchClock(clock, START, actions, 0, 2));
  });

  it("counts the commands that close a turn", () => {
    expect(commandEndsTurn({ type: "endTurn" })).toBe(true);
    expect(commandEndsTurn({ type: "timeout" })).toBe(true);
    expect(
      commandEndsTurn({ type: "build", position: { x: 1, y: 1 }, unit_type: "infantry" }),
    ).toBe(false);
  });
});

describe("clock formatting", () => {
  it("keeps the two largest units that carry the span", () => {
    expect(formatClockDuration(7 * DAY)).toBe("7d");
    expect(formatClockDuration(2 * DAY + 5 * HOUR)).toBe("2d 5h");
    expect(formatClockDuration(5 * HOUR + 30 * MINUTE)).toBe("5h 30m");
    expect(formatClockDuration(3 * MINUTE)).toBe("3m");
    expect(formatClockDuration(90_000)).toBe("1m 30s");
    expect(formatClockDuration(9_000)).toBe("9s");
  });

  it("reads a spent clock as no time at all", () => {
    expect(formatClockDuration(0)).toBe("0s");
    expect(formatClockDuration(-5_000)).toBe("0s");
  });

  it("keeps every unit, padded, for the clock a player opens", () => {
    expect(formatClockCountdown(6 * DAY + 4 * HOUR + 7 * MINUTE + 9_000)).toBe("6d 04h 07m 09s");
    expect(formatClockCountdown(4 * HOUR + 9_000)).toBe("4h 00m 09s");
    expect(formatClockCountdown(3 * MINUTE + 20_000)).toBe("3m 20s");
    expect(formatClockCountdown(9_000)).toBe("9s");
    expect(formatClockCountdown(-1)).toBe("0s");
  });
});

describe("clock presets", () => {
  it("names a match that runs on a pace somebody chose", () => {
    const asyncPace = CLOCK_PRESETS.find((preset) => preset.id === "async");

    expect(asyncPace).toBeDefined();
    expect(findClockPreset(asyncPace!.clock)?.id).toBe("async");
    expect(formatClockSummary(asyncPace!.clock)).toBe("Async · 7d +2d");
  });

  it("opens a live match on five minutes, plus two a turn, held back by nothing", () => {
    const live = CLOCK_PRESETS.find((preset) => preset.id === "live");

    expect(live?.clock.initialMs).toBe(5 * MINUTE);
    expect(live?.clock.incrementMs).toBe(2 * MINUTE);
    expect(isBankUncapped(live!.clock)).toBe(true);
    // A ceiling no match reaches is not a term of the game, so it is not read
    // out as one.
    expect(formatClockSummary(live!.clock)).toBe("Live · 5m +2m");
  });

  it("reads terms nobody named as terms alone", () => {
    const clock = { initialMs: 3 * DAY, incrementMs: HOUR, maxBankMs: 5 * DAY };

    expect(findClockPreset(clock)).toBeNull();
    expect(formatClockSummary(clock)).toBe("3d +1h, up to 5d");
  });

  it("gives every preset a bank its ceiling can hold", () => {
    for (const preset of CLOCK_PRESETS) {
      expect(preset.clock.maxBankMs).toBeGreaterThanOrEqual(preset.clock.initialMs);
    }
  });
});

describe("clock pressure", () => {
  it("measures the warning against the bank the match opened on", () => {
    const opening = 7 * DAY;

    expect(clockPressure(opening, opening)).toBe("steady");
    expect(clockPressure(DAY, opening)).toBe("low");
    expect(clockPressure(4 * HOUR, opening)).toBe("critical");
  });

  it("keeps a floor, so a short clock still warns in time to act", () => {
    const opening = 5 * MINUTE;

    expect(clockPressure(4 * MINUTE, opening)).toBe("steady");
    expect(clockPressure(90_000, opening)).toBe("low");
    expect(clockPressure(20_000, opening)).toBe("critical");
  });

  it("redraws only as often as the readout can change", () => {
    expect(clockTickMs(30_000)).toBe(1_000);
    expect(clockTickMs(5 * HOUR)).toBe(15_000);
    expect(clockTickMs(7 * DAY)).toBe(MINUTE);
  });
});

describe("what one seat has left", () => {
  const state = {
    activeSlot: 1,
    banksMs: { 0: 2 * DAY, 1: 5 * DAY },
    deadlineAt: START + 5 * DAY,
  };

  it("counts the open turn down and leaves every other bank still", () => {
    expect(seatRemainingMs(state, 1, START + DAY)).toBe(4 * DAY);
    expect(seatRemainingMs(state, 0, START + DAY)).toBe(2 * DAY);
  });

  it("floors a seat that ran out, and a seat nobody recorded", () => {
    expect(seatRemainingMs(state, 1, START + 9 * DAY)).toBe(0);
    expect(seatRemainingMs(state, 7, START)).toBe(0);
  });
});

describe("formatTurnRemaining", () => {
  it("reads out the time a seat has left", () => {
    expect(formatTurnRemaining(90_000)).toBe("1m 30s left");
    expect(formatTurnRemaining(2 * 86_400_000)).toBe("2d left");
  });

  it("names a deadline that has passed instead of counting it to nothing", () => {
    // The clock is only enforced when the match wakes, so a turn is regularly
    // read after its deadline and must not report as one with time left.
    expect(formatTurnRemaining(0)).toBe("Overdue");
    expect(formatTurnRemaining(-1)).toBe("Overdue");
    expect(formatTurnRemaining(-5 * 86_400_000)).toBe("Overdue");
  });
});

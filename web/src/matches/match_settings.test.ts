import { describe, expect, it } from "vitest";
import {
  defaultMatchClock,
  MAX_CLOCK_MS,
  matchClockSchema,
  matchCreateRequestSchema,
  matchSettingsSchema,
} from "./schemas.ts";

const DAY_MS = 24 * 60 * 60 * 1000;

const validMap = { mapId: "000000000001", revision: 1 };

describe("match clock settings", () => {
  it("starts a match on seven days, plus two a turn, banked to seven", () => {
    expect(defaultMatchClock).toEqual({
      initialMs: 7 * DAY_MS,
      incrementMs: 2 * DAY_MS,
      maxBankMs: 7 * DAY_MS,
    });
  });

  it("refuses match settings that name no clock", () => {
    expect(matchSettingsSchema.safeParse({ fogEnabled: false, startingFunds: 0 }).success).toBe(
      false,
    );
  });

  it("refuses a bank ceiling below the starting time", () => {
    const clock = { initialMs: 7 * DAY_MS, incrementMs: DAY_MS, maxBankMs: 2 * DAY_MS };

    expect(matchClockSchema.safeParse(clock).success).toBe(false);
  });

  it("refuses a clock that runs past the ceiling on any setting", () => {
    const past = MAX_CLOCK_MS + 1;

    expect(matchClockSchema.safeParse({ ...defaultMatchClock, initialMs: past }).success).toBe(
      false,
    );
    expect(matchClockSchema.safeParse({ ...defaultMatchClock, incrementMs: past }).success).toBe(
      false,
    );
    expect(matchClockSchema.safeParse({ ...defaultMatchClock, maxBankMs: past }).success).toBe(
      false,
    );
  });

  it("takes a clock with no increment, which is a plain countdown", () => {
    const clock = { initialMs: DAY_MS, incrementMs: 0, maxBankMs: DAY_MS };

    expect(matchClockSchema.safeParse(clock).success).toBe(true);
  });

  it("makes a new match name its clock", () => {
    const request = {
      name: "Riverside Duel",
      map: validMap,
      isPrivate: false,
      settings: { fogEnabled: false, startingFunds: 0 },
    };

    expect(matchCreateRequestSchema.safeParse(request).success).toBe(false);
    expect(
      matchCreateRequestSchema.safeParse({
        ...request,
        settings: { ...request.settings, clock: defaultMatchClock },
      }).success,
    ).toBe(true);
  });
});

describe("seats a match is made with", () => {
  const request = {
    name: "Riverside Duel",
    map: validMap,
    isPrivate: false,
    settings: { fogEnabled: false, startingFunds: 0, clock: defaultMatchClock },
  };

  it("opens a lobby of people when the host seats nobody", () => {
    const parsed = matchCreateRequestSchema.parse(request);
    expect(parsed.aiSeats).toEqual([]);
  });

  it("takes the opponents the host seated", () => {
    const parsed = matchCreateRequestSchema.parse({
      ...request,
      aiSeats: [{ slotIndex: 1, profileId: "ai-hard-v1" }],
    });
    expect(parsed.aiSeats).toEqual([{ slotIndex: 1, profileId: "ai-hard-v1" }]);
  });

  it("refuses an opponent this build has no profile for", () => {
    const parsed = matchCreateRequestSchema.safeParse({
      ...request,
      aiSeats: [{ slotIndex: 1, profileId: "ai-unbeatable-v9" }],
    });
    expect(parsed.success).toBe(false);
  });

  /**
   * Two opponents in one seat is a lobby that cannot be built, and the insert
   * would fail on the seat's own primary key. Saying so here is a message the
   * host can read.
   */
  it("refuses two opponents in the same seat", () => {
    const parsed = matchCreateRequestSchema.safeParse({
      ...request,
      aiSeats: [
        { slotIndex: 1, profileId: "ai-easy-v1" },
        { slotIndex: 1, profileId: "ai-hard-v1" },
      ],
    });
    expect(parsed.success).toBe(false);
  });
});

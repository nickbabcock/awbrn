import { describe, expect, it } from "vitest";
import { ownedSlotIndices, selectOwnedPerspectiveSlot } from "./hotseat.ts";
import {
  defaultMatchClock,
  matchMutationRequestSchema,
  matchSettingsSchema,
  type MatchSetup,
} from "./schemas.ts";

const setup = {
  players: [{ userId: "alice" }, { userId: "bob" }, { userId: "alice" }, { userId: "carol" }],
} as MatchSetup;

describe("hotseat perspective selection", () => {
  it("finds every seat owned by an account", () => {
    expect(ownedSlotIndices(setup, "alice")).toEqual([0, 2]);
    expect(ownedSlotIndices(setup, "nobody")).toEqual([]);
  });

  it("uses an active owned army", () => {
    expect(selectOwnedPerspectiveSlot([0, 2], 2, [{ player: 0 }])).toBe(2);
  });

  it("keeps the most recently acting owned army during a remote turn", () => {
    expect(
      selectOwnedPerspectiveSlot([0, 2], 3, [
        { player: 0 },
        { player: 1 },
        { player: 2 },
        { player: 3 },
      ]),
    ).toBe(2);
  });

  it("falls back to the lowest owned seat before either army has acted", () => {
    expect(selectOwnedPerspectiveSlot([0, 2], 1, [])).toBe(0);
  });

  it("returns no player perspective for a spectator", () => {
    expect(selectOwnedPerspectiveSlot([], 1, [])).toBeNull();
  });
});

describe("hotseat schemas", () => {
  it("keeps existing match settings non-hotseat by default", () => {
    expect(
      matchSettingsSchema.parse({
        fogEnabled: false,
        startingFunds: 1000,
        clock: defaultMatchClock,
      }),
    ).toEqual({
      fogEnabled: false,
      startingFunds: 1000,
      hotseatEnabled: false,
      bannedCoIds: [],
      clock: defaultMatchClock,
    });
  });

  it("requires seat-specific lobby updates and leaves", () => {
    expect(matchMutationRequestSchema.safeParse({ action: "leave", slotIndex: 2 }).success).toBe(
      true,
    );
    expect(matchMutationRequestSchema.safeParse({ action: "leave" }).success).toBe(false);
    expect(
      matchMutationRequestSchema.safeParse({
        action: "updateParticipant",
        slotIndex: 2,
        ready: true,
      }).success,
    ).toBe(true);
  });
});

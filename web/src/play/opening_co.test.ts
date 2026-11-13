import { describe, expect, it } from "vitest";
import type { MyMatchSummary } from "#/matches/schemas.ts";
import { FIRST_CO_ID, openingCoId } from "./opening_co.ts";

const NONE: ReadonlySet<number> = new Set();

function match(matchId: string, coId: number | null, updatedAt: string): MyMatchSummary {
  return {
    matchId,
    viewerParticipants: [
      { slotIndex: 0, factionId: 1, coId, ready: false, joinedAt: updatedAt, updatedAt },
    ],
  } as MyMatchSummary;
}

describe("openingCoId", () => {
  it("opens on Andy for a player with no commander on record", () => {
    expect(openingCoId([], NONE, "here")).toBe(FIRST_CO_ID);
  });

  it("opens on the commander the player chose last", () => {
    const matches = [
      match("old", 5, "2026-01-01T00:00:00.000Z"),
      match("new", 9, "2026-02-01T00:00:00.000Z"),
      match("blank", null, "2026-03-01T00:00:00.000Z"),
    ];
    expect(openingCoId(matches, NONE, "here")).toBe(9);
  });

  it("skips this match and banned commanders", () => {
    const matches = [
      match("here", 7, "2026-03-01T00:00:00.000Z"),
      match("banned", 9, "2026-02-01T00:00:00.000Z"),
    ];
    expect(openingCoId(matches, new Set([9]), "here")).toBe(FIRST_CO_ID);
    expect(openingCoId([], new Set([FIRST_CO_ID]), "here")).not.toBe(FIRST_CO_ID);
  });
});

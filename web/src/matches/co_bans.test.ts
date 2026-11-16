import { describe, expect, it } from "vitest";
import { coRoster } from "#/co_roster.ts";
import { canonicalBans, setBans } from "./co_bans.ts";
import { bannedCoIdsSchema, defaultMatchClock, matchSettingsSchema } from "./schemas.ts";

const [first, second, third] = coRoster;

describe("CO bans", () => {
  it("writes one list however the host pressed the board", () => {
    expect(bannedCoIdsSchema.parse([second!.awbwId, first!.awbwId, second!.awbwId])).toEqual(
      [first!.awbwId, second!.awbwId].sort((left, right) => left - right),
    );
  });

  it("drops an id no CO in this build answers to", () => {
    expect(bannedCoIdsSchema.parse([third!.awbwId, 9_999])).toEqual([third!.awbwId]);
  });

  it("refuses a match with no CO left to choose", () => {
    const everyCo = coRoster.map((co) => co.awbwId);
    expect(bannedCoIdsSchema.safeParse(everyCo).success).toBe(false);
    expect(bannedCoIdsSchema.safeParse(everyCo.slice(1)).success).toBe(true);
  });

  it("reads a match created before COs could be banned as banning none", () => {
    expect(
      matchSettingsSchema.parse({
        fogEnabled: true,
        startingFunds: 0,
        hotseatEnabled: true,
        clock: defaultMatchClock,
      }).bannedCoIds,
    ).toEqual([]);
  });
});

describe("per-seat CO bans", () => {
  const ids = coRoster.map((co) => co.awbwId);
  const [a, b, c, d] = ids as [number, number, number, number];

  it("stores each seat's list sorted, once each and apart from All", () => {
    expect(canonicalBans({ bannedCoIds: [a], seatBannedCoIds: [[c, a, b, b], []] }, 2)).toEqual({
      bannedCoIds: [a],
      seatBannedCoIds: [[b, c].sort((left, right) => left - right), []],
    });
  });

  it("moves a CO every seat bans into All as the match is made", () => {
    expect(canonicalBans({ bannedCoIds: [], seatBannedCoIds: [[b, c], [b], [b, d]] }, 3)).toEqual({
      bannedCoIds: [b],
      seatBannedCoIds: [[c], [], [d]],
    });
  });

  it("stores no seat lists when no seat adds a ban", () => {
    expect(
      canonicalBans({ bannedCoIds: [a], seatBannedCoIds: [[a], []] }, 2).seatBannedCoIds,
    ).toBeUndefined();
  });

  it("leaves each seat its last CO, whatever the press", () => {
    const [x, y, ...rest] = ids as [number, number, ...number[]];
    const draft = { all: rest, seats: [[y], []] };
    // Seat 1 takes the ban; seat 0 would have nothing left, so it does not.
    const pressed = setBans(draft, [0, 1], [x], true);
    expect(pressed.seats).toEqual([[y], [x]]);
    // All reaches seat 1 too, whose last CO is now y.
    expect(setBans(pressed, "all", [y], true)).toEqual(pressed);
  });
});

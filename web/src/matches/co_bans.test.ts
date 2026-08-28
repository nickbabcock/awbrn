import { describe, expect, it } from "vitest";
import { coRoster } from "#/co_roster.ts";
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

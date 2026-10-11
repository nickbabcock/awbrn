import { describe, expect, it } from "vitest";
import { coRoster } from "#/co_roster.ts";
import { coTierBands, coTierListFor, coTierOf, type CoTierList } from "#/co_tiers.ts";

const lists: CoTierList[] = ["standard", "fog", "ffa"];

describe("the CO tier lists", () => {
  it.each(lists)("rate every CO in the %s list", (list) => {
    for (const co of coRoster) {
      expect(coTierOf(co.key, list), co.key).not.toBeNull();
    }
  });

  it.each(lists)("put each CO in exactly one %s band", (list) => {
    const keys = coTierBands(list).flatMap((band) => band.cos.map((co) => co.key));

    expect(keys).toHaveLength(coRoster.length);
    expect(new Set(keys).size).toBe(coRoster.length);
  });

  it("puts Colin above Adder", () => {
    expect(coTierOf("colin", "standard")).toBe(0);
    expect(coTierOf("adder", "standard")).toBe(4);
  });
});

describe("choosing a tier list for a match", () => {
  it("takes the fog list for a two-army match in fog", () => {
    expect(coTierListFor({ fogEnabled: true, playerCount: 2 })).toBe("fog");
  });

  it("takes the standard list for a clear two-army match", () => {
    expect(coTierListFor({ fogEnabled: false, playerCount: 2 })).toBe("standard");
  });

  it("takes the free-for-all list for more than two armies, fog or not", () => {
    expect(coTierListFor({ fogEnabled: true, playerCount: 4 })).toBe("ffa");
    expect(coTierListFor({ fogEnabled: false, playerCount: 3 })).toBe("ffa");
  });
});

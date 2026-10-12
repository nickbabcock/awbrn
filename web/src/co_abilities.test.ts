import { describe, expect, it } from "vitest";
import powers from "../../spec/rulesets/awbw/2026-07-10/commander-powers.json";
import { coAbilities } from "#/co_abilities.ts";
import { coRoster } from "#/co_roster.ts";

type SpecPower = { stars: number };
type SpecCommander = { cop?: SpecPower; scop?: SpecPower };
const specCommanders = powers.commanders as Record<string, SpecCommander>;

describe("CO abilities", () => {
  it("describes every commander on the roster", () => {
    for (const co of coRoster) {
      expect(coAbilities[co.key], co.key).toBeDefined();
    }
  });

  it("matches the spec's star costs for each power", () => {
    for (const co of coRoster) {
      const specCo = specCommanders[co.key];
      const abilities = coAbilities[co.key];
      if (!abilities || !specCo) continue;

      expect(abilities.power?.stars ?? null, `${co.key} power`).toBe(specCo.cop?.stars ?? null);
      expect(abilities.superPower.stars, `${co.key} super power`).toBe(specCo.scop?.stars);
    }
  });

  it("gives a commander without a COP no power readout", () => {
    expect(coAbilities["von-bolt"]?.power).toBeNull();
  });
});

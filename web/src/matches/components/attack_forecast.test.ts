import { describe, expect, it } from "vitest";
import { engagementLabel, formatBracket } from "#/matches/components/attack_forecast.ts";
import type { AttackForecast } from "#/wasm/awbrn_wasm.js";

function forecast(overrides: Partial<AttackForecast> = {}): AttackForecast {
  return {
    target: { type: "unit", unit: "tank", name: "Tank", factionCode: "bm", health: 7 },
    damage: { low: 65, high: 75 },
    counter: { low: 18, high: 22 },
    counterFirst: false,
    destroys: false,
    mayDestroy: false,
    ...overrides,
  };
}

describe("formatBracket", () => {
  it("reads a spread as a range", () => {
    expect(formatBracket({ low: 65, high: 75 })).toBe("65 – 75%");
  });

  // A commander with no luck rules produces a bracket of one value, which is a
  // common case rather than an edge one, and writing it as a range is noise.
  it("collapses a bracket of one value to one number", () => {
    expect(formatBracket({ low: 100, high: 100 })).toBe("100%");
  });
});

describe("engagementLabel", () => {
  it("names the army in full rather than by the code the sprite colours", () => {
    expect(engagementLabel("Fire", forecast())).toBe(
      "Fire Blue Moon Tank at 7 HP, dealing 65 – 75%, taking 18 – 22% back",
    );
  });

  // No reply and a reply that happens to do nothing are different facts.
  it("says no reply rather than a counter of zero", () => {
    expect(engagementLabel("Fire", forecast({ counter: undefined }))).toBe(
      "Fire Blue Moon Tank at 7 HP, dealing 65 – 75%, with no reply",
    );
  });

  // The figure is uncapped, the way AWBW reports it, so an overkill is visible
  // as one rather than collapsing onto 100%.
  it("distinguishes a certain kill from a possible one", () => {
    expect(
      engagementLabel(
        "Fire",
        forecast({ damage: { low: 104, high: 116 }, counter: undefined, destroys: true }),
      ),
    ).toBe("Fire Blue Moon Tank at 7 HP, dealing 104 – 116%, destroying it");
    expect(engagementLabel("Fire", forecast({ mayDestroy: true }))).toBe(
      "Fire Blue Moon Tank at 7 HP, dealing 65 – 75%, possibly destroying it, taking 18 – 22% back",
    );
  });

  // A commander who answers first inverts the exchange, and a reader who cannot
  // see the label has no other way to learn that.
  it("says a pre-emptive counter comes first", () => {
    expect(engagementLabel("Fire", forecast({ counterFirst: true }))).toBe(
      "Fire Blue Moon Tank at 7 HP, dealing 65 – 75%, taking 18 – 22% first",
    );
  });

  it("names a destructible tile without an army", () => {
    expect(
      engagementLabel(
        "Fire",
        forecast({ target: { type: "tile", name: "Pipe Seam" }, counter: undefined }),
      ),
    ).toBe("Fire Pipe Seam, dealing 65 – 75%, with no reply");
  });
});

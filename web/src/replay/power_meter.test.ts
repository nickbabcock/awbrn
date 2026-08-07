import { describe, expect, it } from "vitest";
import { canActivatePower, readPowerMeter } from "./power_meter.ts";
import type { PlayerRosterEntry } from "#/wasm/awbrn_wasm.js";

/** A roster entry with only the fields the meter reads. */
function entry(power: Partial<PlayerRosterEntry>): PlayerRosterEntry {
  return {
    playerId: 0,
    userId: 0,
    turnOrder: 0,
    team: undefined,
    eliminated: false,
    actualFactionCode: "os",
    actualFactionName: "Orange Star",
    displayFactionCode: "os",
    displayFactionName: "Orange Star",
    factionCode: "os",
    factionName: "Orange Star",
    coKey: undefined,
    coName: undefined,
    tagCoKey: undefined,
    tagCoName: undefined,
    powerCharge: undefined,
    copCost: undefined,
    scopCost: undefined,
    powerStarCharge: undefined,
    activePower: undefined,
    stats: {
      funds: undefined,
      income: undefined,
      unitCount: undefined,
      unitValue: undefined,
      properties: undefined,
      comTowers: undefined,
    },
    ...power,
  };
}

describe("readPowerMeter", () => {
  it("counts stars from the costs AWVM reports", () => {
    const meter = readPowerMeter(
      entry({ powerCharge: 0, copCost: 27_000, scopCost: 54_000, powerStarCharge: 9_000 }),
    );

    expect(meter?.cop?.stars).toBe(3);
    expect(meter?.scop?.stars).toBe(6);
    expect(meter?.totalStars).toBe(6);
  });

  it("keeps the star count steady after a power has raised the price", () => {
    // One use scales every cost by 20%, so the bar must stay six stars long.
    const meter = readPowerMeter(
      entry({ powerCharge: 0, copCost: 32_400, scopCost: 64_800, powerStarCharge: 10_800 }),
    );

    expect(meter?.cop?.stars).toBe(3);
    expect(meter?.scop?.stars).toBe(6);
  });

  it("reports the fraction of the star in progress", () => {
    const meter = readPowerMeter(
      entry({ powerCharge: 22_500, copCost: 27_000, scopCost: 54_000, powerStarCharge: 9_000 }),
    );

    expect(meter?.charged).toBe(2.5);
    expect(meter?.level).toBe("charging");
    expect(meter?.cop?.remaining).toBe(4_500);
  });

  it("names the strongest power the charge pays for", () => {
    const costs = { copCost: 27_000, scopCost: 54_000, powerStarCharge: 9_000 };

    expect(readPowerMeter(entry({ powerCharge: 27_000, ...costs }))?.level).toBe("cop");
    expect(readPowerMeter(entry({ powerCharge: 53_999, ...costs }))?.level).toBe("cop");
    expect(readPowerMeter(entry({ powerCharge: 54_000, ...costs }))?.level).toBe("scop");
  });

  it("does not overflow the bar once the meter is full", () => {
    const meter = readPowerMeter(
      entry({ powerCharge: 90_000, copCost: 27_000, scopCost: 54_000, powerStarCharge: 9_000 }),
    );

    expect(meter?.charged).toBe(6);
  });

  it("draws a super-only meter for a CO with no normal power", () => {
    const meter = readPowerMeter(
      entry({ powerCharge: 9_000, scopCost: 90_000, powerStarCharge: 9_000 }),
    );

    expect(meter?.cop).toBeNull();
    expect(meter?.totalStars).toBe(10);
  });

  it("draws nothing when the CO has no power to charge toward", () => {
    expect(readPowerMeter(entry({ powerCharge: 0, powerStarCharge: 9_000 }))).toBeNull();
    expect(readPowerMeter(entry({ powerCharge: 0, copCost: 27_000 }))).toBeNull();
  });
});

describe("canActivatePower", () => {
  const costs = { copCost: 27_000, scopCost: 54_000, powerStarCharge: 9_000 };

  it("becomes available at the normal-power threshold", () => {
    expect(canActivatePower(entry({ powerCharge: 26_999, ...costs }), "cop")).toBe(false);
    expect(canActivatePower(entry({ powerCharge: 27_000, ...costs }), "cop")).toBe(true);
  });

  it("becomes available at the super-power threshold", () => {
    expect(canActivatePower(entry({ powerCharge: 53_999, ...costs }), "scop")).toBe(false);
    expect(canActivatePower(entry({ powerCharge: 54_000, ...costs }), "scop")).toBe(true);
  });

  it("is unavailable while a power is active", () => {
    const player = entry({ powerCharge: 54_000, activePower: "scop", ...costs });
    expect(canActivatePower(player, "cop")).toBe(false);
    expect(canActivatePower(player, "scop")).toBe(false);
  });
});

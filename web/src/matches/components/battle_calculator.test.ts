import { describe, expect, it } from "vitest";
import type {
  BattleResult,
  CatalogUnit,
  PlayerRosterEntry,
  PlayerRosterSnapshot,
} from "#/wasm/awbrn_wasm.js";
import {
  barsToPoints,
  commanderFrom,
  defaultTerrain,
  engagementLabel,
  formatDamage,
  formatFunds,
  formatFundsBracket,
  formatNet,
  impossibleLabel,
  newFighter,
  pointsToBars,
  retypeFighter,
  seatsFrom,
  sideFrom,
} from "./battle_calculator.ts";

function unit(overrides: Partial<CatalogUnit> = {}): CatalogUnit {
  return {
    unit: "tank",
    name: "Tank",
    cost: 7000,
    domain: "ground",
    maxAmmo: 9,
    isIndirect: false,
    ...overrides,
  };
}

function entry(overrides: Partial<PlayerRosterEntry> = {}): PlayerRosterEntry {
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
    ...overrides,
  };
}

function roster(players: PlayerRosterEntry[], activePlayerId?: number): PlayerRosterSnapshot {
  return {
    matchId: 1,
    mapId: 1,
    day: 4,
    activePlayerId,
    weather: "rain",
    players,
  };
}

function result(overrides: Partial<BattleResult> = {}): BattleResult {
  return {
    weapon: "ammo",
    damage: { low: 55, high: 64 },
    counter: { low: 18, high: 22 },
    counterFirst: false,
    destroys: false,
    mayDestroy: false,
    valueDealt: { low: 3850, high: 4480 },
    valueTaken: { low: 1260, high: 1540 },
    counterSteps: [],
    targetValue: 7000,
    net: { low: 2310, high: 3220 },
    ...overrides,
  };
}

describe("health", () => {
  it("rounds a partial bar up, the way the board draws it", () => {
    expect(pointsToBars(61)).toBe(7);
    expect(pointsToBars(70)).toBe(7);
    expect(pointsToBars(100)).toBe(10);
  });

  it("never reports a standing unit as gone", () => {
    expect(pointsToBars(1)).toBe(1);
  });

  it("converts a bar back to the points the reducer counts", () => {
    expect(barsToPoints(7)).toBe(70);
  });
});

describe("fighters", () => {
  it("opens a naval unit on water rather than on a plain", () => {
    expect(defaultTerrain("sea")).toBe("sea");
    expect(defaultTerrain("ground")).toBe("plain");
  });

  it("starts a new unit whole and loaded", () => {
    const fighter = newFighter(unit());
    expect(fighter.health).toBe(100);
    expect(fighter.ammo).toBe(9);
  });

  it("gives an unarmed unit no magazine to edit", () => {
    expect(newFighter(unit({ unit: "apc", maxAmmo: 0 })).ammo).toBeUndefined();
  });

  it("keeps condition and ground when the unit is swapped, but not the magazine", () => {
    const damaged = { unit: "tank" as const, health: 60, ammo: 2, terrain: "mountain" as const };
    const swapped = retypeFighter(damaged, unit({ unit: "neo-tank", maxAmmo: 6 }));

    expect(swapped.health).toBe(60);
    expect(swapped.terrain).toBe("mountain");
    // Six is what a Neo Tank carries; inheriting two would have been a lie in
    // the other direction, and inheriting nine an impossibility.
    expect(swapped.ammo).toBe(6);
  });
});

describe("seating", () => {
  it("puts the viewer in the attacking seat", () => {
    const seats = seatsFrom(roster([entry({ playerId: 0 }), entry({ playerId: 1 })], 1), 0);
    expect(seats.attacker?.playerId).toBe(0);
    expect(seats.defender?.playerId).toBe(1);
  });

  it("falls back to whoever is acting when the viewer is a spectator", () => {
    const seats = seatsFrom(roster([entry({ playerId: 0 }), entry({ playerId: 1 })], 1), null);
    expect(seats.attacker?.playerId).toBe(1);
    expect(seats.defender?.playerId).toBe(0);
  });

  it("does not seat an ally as the defender", () => {
    const seats = seatsFrom(
      roster(
        [
          entry({ playerId: 0, team: "a" }),
          entry({ playerId: 1, team: "a" }),
          entry({ playerId: 2, team: "b" }),
        ],
        0,
      ),
      0,
    );
    expect(seats.defender?.playerId).toBe(2);
  });

  it("has no seats before a board reports", () => {
    expect(seatsFrom(null, null)).toEqual({ attacker: null, defender: null });
  });
});

describe("side context", () => {
  it("reads the army's own commander, money and holdings", () => {
    const side = sideFrom(
      entry({
        coKey: "colin",
        activePower: "scop",
        stats: {
          funds: 24_000,
          income: 9000,
          unitCount: 12,
          unitValue: 40_000,
          properties: 9,
          comTowers: 2,
        },
      }),
    );

    expect(side).toEqual({
      commander: "colin",
      power: "scop",
      funds: 24_000,
      properties: 9,
      comTowers: 2,
    });
  });

  it("keeps withheld figures hidden", () => {
    expect(sideFrom(entry())).toMatchObject({
      funds: undefined,
      properties: undefined,
      comTowers: undefined,
    });
  });

  it("preserves a reported zero", () => {
    expect(
      sideFrom(entry({ stats: { ...entry().stats, funds: 0, properties: 0, comTowers: 0 } })),
    ).toMatchObject({ funds: 0, properties: 0, comTowers: 0 });
  });

  it("treats the portrait sheet's placeholder as no commander", () => {
    expect(commanderFrom("no-co")).toBeUndefined();
    expect(commanderFrom(undefined)).toBeUndefined();
    expect(commanderFrom("hawke")).toBe("hawke");
  });
});

describe("wording", () => {
  it("collapses a range of one value to a single figure", () => {
    expect(formatDamage({ low: 55, high: 55 })).toBe("55%");
    expect(formatDamage({ low: 55, high: 64 })).toBe("55 – 64%");
  });

  it("groups funds the way money is read", () => {
    expect(formatFunds(12_400)).toBe("12,400");
    expect(formatFundsBracket({ low: 3850, high: 3850 })).toBe("3,850");
    expect(formatFundsBracket({ low: 3850, high: 4480 })).toBe("3,850 – 4,480");
  });

  it("always signs the net, on both ends", () => {
    expect(formatNet({ low: 2310, high: 3220 })).toBe("+2,310 – +3,220");
    expect(formatNet({ low: -900, high: -900 })).toBe("−900");
    // The bad end losing money and the good end making it is exactly the case a
    // player is deciding on, so both signs appear in one figure.
    expect(formatNet({ low: -900, high: 1200 })).toBe("−900 – +1,200");
  });

  it("says why a pairing has no numbers", () => {
    expect(impossibleLabel("unarmed")).toBe("Attacker has no weapon entry for this target");
    expect(impossibleLabel("no-weapon")).toBe("Cannot reach this target");
  });
});

describe("the row said as a sentence", () => {
  const target = { unit: "tank" as const, health: 100, ammo: 9, terrain: "plain" as const };

  it("carries both halves of the exchange and the net", () => {
    const label = engagementLabel("Md Tank", "Tank", target, result(), undefined);
    expect(label).toContain("Tank at 10 HP");
    expect(label).toContain("dealing 55 – 64%");
    expect(label).toContain("worth 3,850 – 4,480 funds");
    expect(label).toContain("taking 18 – 22% back");
    expect(label).toContain("Net +2,310 – +3,220 funds");
  });

  it("distinguishes no reply from a reply that lands nothing", () => {
    const silent = engagementLabel(
      "Artillery",
      "Tank",
      target,
      result({ counter: undefined, valueTaken: undefined }),
      undefined,
    );
    expect(silent).toContain("with no reply");

    const zero = engagementLabel(
      "Artillery",
      "Tank",
      target,
      result({ counter: { low: 0, high: 4 }, valueTaken: { low: 0, high: 280 } }),
      undefined,
    );
    expect(zero).toContain("taking 0 – 4% back");
    expect(zero).not.toContain("no reply");
  });

  it("marks a commander who answers first, because the order is not visible", () => {
    expect(
      engagementLabel("Infantry", "Mech", target, result({ counterFirst: true }), undefined),
    ).toContain("first");
  });

  it("says a kill without waiting for the reader to compare two numbers", () => {
    expect(
      engagementLabel(
        "Bomber",
        "Tank",
        target,
        result({ destroys: true, counter: undefined }),
        undefined,
      ),
    ).toContain("destroying it");
    expect(
      engagementLabel("Tank", "Recon", target, result({ mayDestroy: true }), undefined),
    ).toContain("possibly destroying it");
  });

  it("reports an impossible pairing rather than an empty row", () => {
    expect(engagementLabel("Anti-Air", "Battleship", target, undefined, "no-weapon")).toContain(
      "Cannot reach this target",
    );
  });
});

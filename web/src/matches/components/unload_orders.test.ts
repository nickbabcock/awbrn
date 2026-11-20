import { describe, expect, it } from "vitest";
import {
  aimAnnouncement,
  dockRows,
  menuClearance,
  unloadLabel,
  type UnloadOption,
} from "#/matches/components/unload_orders.ts";
import type { CargoBadge, UnitActionOption } from "#/wasm/awbrn_wasm.js";

const infantry: CargoBadge = {
  unit: "infantry",
  name: "Infantry",
  factionCode: "os",
  health: 10,
  lowFuel: false,
  lowAmmo: false,
};

function unload(cargoId: number, tiles: number): UnloadOption {
  return {
    name: "Unload Infantry",
    action: {
      type: "unload",
      cargo_id: cargoId,
      tiles: Array.from({ length: tiles }, (_, x) => ({ x, y: 0 })),
    },
    cargo: infantry,
  };
}

const wait: UnitActionOption = { name: "Wait", action: { type: "move", action: { type: "wait" } } };

describe("unloadLabel", () => {
  it("names the cargo and how many tiles it may land on", () => {
    expect(unloadLabel(unload(1, 3))).toBe("Unload Infantry, 3 tiles");
    expect(unloadLabel(unload(1, 1))).toBe("Unload Infantry, 1 tile");
  });

  it("tells two of a kind apart by health and marks", () => {
    const worn = {
      ...unload(1, 2),
      cargo: { ...infantry, health: 7, lowFuel: true, lowAmmo: true },
    };
    expect(unloadLabel(worn)).toBe("Unload Infantry at 7 HP, low fuel, low ammo, 2 tiles");
  });
});

describe("aimAnnouncement", () => {
  it("says where the cargo lands and what the tile gives it", () => {
    expect(
      aimAnnouncement(infantry, { row: 0, x: 12, y: 9, terrainName: "Plain", defenseStars: 1 }),
    ).toBe("Infantry to 12, 9, Plain, 1 star");
  });
});

describe("dockRows", () => {
  it("holds two rows with cargo aboard: the Unload keys, then the other orders", () => {
    const supply: UnitActionOption = {
      name: "Supply",
      action: { type: "move", action: { type: "supply" } },
    };
    expect(dockRows([supply, unload(1, 2), unload(2, 1), wait])).toEqual([
      [1, 2],
      [0, 3],
    ]);
  });

  it("starts another row rather than squeeze a fourth key in", () => {
    const repair: UnitActionOption = {
      name: "Repair",
      action: { type: "move", action: { type: "repair", target_id: 1 } },
    };
    const remove: UnitActionOption = { name: "Delete", action: { type: "delete" } };
    expect(dockRows([repair, repair, unload(1, 2), wait, remove])).toEqual([[2], [0, 1, 3], [4]]);
  });
});

describe("menuClearance", () => {
  it("clears one tile more for orders about a neighboring tile", () => {
    const fire: UnitActionOption = {
      name: "Fire",
      action: { type: "move", action: { type: "attack", target: { x: 1, y: 0 } } },
      forecast: {} as UnitActionOption["forecast"],
    };
    expect(menuClearance(32, [wait])).toBe(24);
    expect(menuClearance(32, [unload(1, 1), wait])).toBe(56);
    expect(menuClearance(32, [fire, wait])).toBe(56);
  });
});

import { describe, expect, it } from "vitest";
import type {
  EditorArmy,
  EditorPalette,
  EditorStateChanged,
  PaletteCell,
} from "#/wasm/awbrn_wasm.js";
import {
  isDirty,
  nameProblem,
  problems,
  resizeKept,
  rosterOfSize,
  saveFailure,
  saveKey,
  savesAsNewMap,
  stepBrush,
  SYMMETRY_MODES,
  trayCells,
} from "./map_editor.ts";
import { MAP_SAVE_REFUSALS } from "./schemas.ts";

function army(overrides: Partial<EditorArmy> & { faction: string }): EditorArmy {
  return {
    hqAt: [{ x: 0, y: 0 }],
    properties: 4,
    production: 2,
    labs: 0,
    units: 0,
    income: 0,
    ...overrides,
  };
}

function state(overrides: Partial<EditorStateChanged> = {}): EditorStateChanged {
  return {
    width: 20,
    height: 16,
    armies: [army({ faction: "os" }), army({ faction: "bm" })],
    neutralProperties: 6,
    units: 0,
    symmetry: "rotate180",
    availableSymmetries: [...SYMMETRY_MODES],
    unevenGroups: [],
    boardIncome: 28_000,
    canUndo: false,
    canRedo: false,
    historyId: 3,
    boardChanged: false,
    brush: { kind: "terrain", terrain: 1 },
    roster: ["os", "bm"],
    ...overrides,
  };
}

function cell(group: PaletteCell["group"], brush: PaletteCell["brush"]): PaletteCell {
  return { brush, name: "Cell", group, defense: 0 };
}

describe("problems", () => {
  it("lists none on a pristine board, whatever it holds", () => {
    expect(problems(state({ armies: [] }), true)).toEqual([]);
  });

  it("finds a seated board playable", () => {
    expect(problems(state(), false)).toEqual([]);
  });

  it("steps tile problems first, then each army's, then every empty seat as one", () => {
    const pair = [
      { x: 5, y: 7 },
      { x: 16, y: 8 },
    ];
    const hqs = [
      { x: 13, y: 5 },
      { x: 13, y: 15 },
    ];
    const list = problems(
      state({
        unevenGroups: [pair],
        armies: [
          army({ faction: "os", production: 0 }),
          army({ faction: "bm", hqAt: [] }),
          army({ faction: "bh", hqAt: hqs }),
        ],
        roster: ["os", "bm", "ge", "yc", "bh"],
      }),
      false,
    );

    expect(list).toEqual([
      { kind: "uneven", tiles: pair },
      { kind: "extra-hq", faction: "bh", tiles: hqs },
      { kind: "no-hq", faction: "bm" },
      { kind: "no-build", faction: "os" },
      { kind: "empty", factions: ["ge", "yc"] },
    ]);
  });

  it("lets an army that holds a lab go without an HQ", () => {
    const armies = [army({ faction: "os" }), army({ faction: "bm", hqAt: [], labs: 1 })];
    expect(problems(state({ armies }), false)).toEqual([]);
  });
});

describe("saving", () => {
  it("is unsaved when the history or the name moved", () => {
    const savedAt = { historyId: 3, name: "Twin Fords" };
    expect(isDirty({ historyId: 3 }, savedAt, "Twin Fords")).toBe(false);
    expect(isDirty({ historyId: 4 }, savedAt, "Twin Fords")).toBe(true);
    expect(isDirty({ historyId: 3 }, savedAt, "Twin Fords II")).toBe(true);
  });

  it("names what a save writes, and is red only while unsaved", () => {
    expect(saveKey("create", false)).toEqual({ label: "Save map", isRed: false, isInert: true });
    expect(saveKey("create", true)).toEqual({ label: "Save map", isRed: true, isInert: false });
    expect(saveKey("revise", false)).toEqual({ label: "No changes", isRed: false, isInert: true });
    expect(saveKey("revise", true)).toEqual({
      label: "Save changes",
      isRed: true,
      isInert: false,
    });
    // A clean fork still writes a map of one's own.
    expect(saveKey("fork", false)).toEqual({
      label: "Save as new map",
      isRed: false,
      isInert: false,
    });
  });

  it("reads each refusal as its recovery", () => {
    expect(saveFailure(new Error(MAP_SAVE_REFUSALS.changed))).toEqual({ kind: "changed" });
    expect(saveFailure(new Error(MAP_SAVE_REFUSALS.missing))).toEqual({ kind: "missing" });
    expect(saveFailure(new Error(MAP_SAVE_REFUSALS.forbidden))).toEqual({ kind: "forbidden" });
    expect(saveFailure(new Error("too many requests; try again in a minute"))).toEqual({
      kind: "busy",
    });
    expect(
      saveFailure(new Error(`${MAP_SAVE_REFUSALS.board}predeployed unit is outside the map`)),
    ).toEqual({ kind: "refused", reason: "predeployed unit is outside the map" });
    const issues = [
      { code: "too_small", path: ["document"], message: "a board is at least 5 tiles on a side" },
    ];
    expect(saveFailure(new Error(JSON.stringify(issues)))).toEqual({
      kind: "refused",
      reason: "a board is at least 5 tiles on a side",
    });
  });

  it("calls anything else a failed save, not a refused board", () => {
    for (const message of ["Failed to fetch", "Unauthorized", "Internal Server Error", "[]"]) {
      expect(saveFailure(new Error(message))).toEqual({ kind: "failed" });
    }
  });

  it("offers a new map only when the revision can no longer be written", () => {
    expect(savesAsNewMap({ kind: "changed" })).toBe(true);
    expect(savesAsNewMap({ kind: "forbidden" })).toBe(true);
    expect(savesAsNewMap({ kind: "busy" })).toBe(false);
    expect(savesAsNewMap({ kind: "refused", reason: "" })).toBe(false);
  });

  it("checks the name before sending it", () => {
    expect(nameProblem("   ", 80)).toBe("Name required");
    expect(nameProblem("x".repeat(81), 80)).toBe("80 characters at most");
    expect(nameProblem(" Twin Fords ", 80)).toBeNull();
  });
});

describe("the brush tray", () => {
  const ground = Array.from({ length: 15 }, (_, terrain) =>
    cell("ground", { kind: "terrain", terrain }),
  );
  const property = (faction?: string) =>
    (["city", "base", "airport", "port", "com-tower", "lab", "hq"] as const)
      .filter((kind) => faction !== undefined || kind !== "hq")
      .map((kind) => cell("property", { kind: "property", property: kind, faction }));
  const army: EditorPalette = {
    terrain: [...ground, ...property("os")],
    units: [
      cell("unit", { kind: "unit", unit: "lander", faction: "os", hp: 10 }),
      cell("unit", { kind: "unit", unit: "infantry", faction: "os", hp: 10 }),
      cell("unit", { kind: "unit", unit: "fighter", faction: "os", hp: 10 }),
    ],
  };
  const neutral: EditorPalette = { terrain: [...ground, ...property()], units: [] };

  it("keeps the eraser in the last row's seventh slot of every tab", () => {
    for (const tab of ["terrain", "buildings", "units"] as const) {
      const cells = trayCells(tab, army, neutral);
      expect(cells.length % 7).toBe(0);
      expect(cells.at(-1)?.brush.kind).toBe(tab === "units" ? "erase-unit" : "erase");
    }
  });

  it("follows the army's buildings with the neutral ones", () => {
    const cells = trayCells("buildings", army, neutral);
    expect(cells.slice(0, 7).every((entry) => entry?.brush.kind === "property")).toBe(true);
    expect(cells[0]?.brush).toMatchObject({ faction: "os" });
    expect(cells[7]?.brush).toMatchObject({ faction: undefined });
  });

  it("files units in build-list order, a row per facility", () => {
    const units = trayCells("units", army, neutral).map((entry) =>
      entry?.brush.kind === "unit" ? entry.brush.unit : (entry?.brush.kind ?? null),
    );
    expect(units.indexOf("infantry")).toBe(0);
    expect(units.indexOf("fighter")).toBe(7);
    expect(units.indexOf("lander")).toBe(14);
  });

  it("steps through a tab and wraps at both ends", () => {
    const cells = ground.slice(0, 3);
    expect(stepBrush(cells, cells[2]!.brush, 1)).toEqual(cells[0]!.brush);
    expect(stepBrush(cells, cells[0]!.brush, -1)).toEqual(cells[2]!.brush);
    expect(stepBrush(cells, { kind: "erase" }, 1)).toEqual(cells[0]!.brush);
  });
});

describe("board setup", () => {
  it("grows the roster in game order and keeps the seats it holds", () => {
    expect(rosterOfSize(["rf", "bd"], 3)).toEqual(["rf", "bd", "os"]);
    expect(rosterOfSize(["os", "bm", "ge"], 1)).toEqual(["os", "bm"]);
  });

  it("keeps the part of the board a resize keeps, as the engine anchors it", () => {
    const from = { width: 18, height: 18 };
    expect(resizeKept(from, { width: 18, height: 16 }, "top-left")).toEqual({
      x0: 0,
      y0: 0,
      x1: 18,
      y1: 16,
    });
    expect(resizeKept(from, { width: 18, height: 15 }, "center")).toEqual({
      x0: 0,
      y0: 1,
      x1: 18,
      y1: 16,
    });
    expect(resizeKept(from, { width: 20, height: 16 }, "bottom-right")).toEqual({
      x0: 0,
      y0: 2,
      x1: 18,
      y1: 18,
    });
  });
});

import { describe, expect, it } from "vitest";
import type { EditorArmy, EditorStateChanged, PaletteCell } from "#/wasm/awbrn_wasm.js";
import {
  boardNotes,
  brushKey,
  EDITOR_SHORTCUTS,
  eraserOfDrawer,
  keyName,
  paletteSections,
  saveBlurb,
  saveLabel,
  stepBrush,
  SYMMETRY_LABELS,
  SYMMETRY_MODES,
} from "./map_editor.ts";

function army(overrides: Partial<EditorArmy> & { faction: string }): EditorArmy {
  return {
    headquarters: 1,
    properties: 4,
    production: 2,
    units: 0,
    ...overrides,
  };
}

function state(overrides: Partial<EditorStateChanged> = {}): EditorStateChanged {
  return {
    revision: 1,
    width: 20,
    height: 16,
    armies: [army({ faction: "os" }), army({ faction: "bm" })],
    neutralProperties: 6,
    units: 0,
    symmetry: "mirror-left-right",
    availableSymmetries: [...SYMMETRY_MODES],
    symmetric: true,
    unevenTiles: 0,
    firstUnevenTile: undefined,
    boardIncome: 28_000,
    incomePerArmy: 14_000,
    canUndo: false,
    canRedo: false,
    brush: { kind: "terrain", terrain: 1 },
    roster: ["os", "bm", "ge", "yc"],
    ...overrides,
  };
}

function cell(group: PaletteCell["group"], brush: PaletteCell["brush"]): PaletteCell {
  return { brush, name: "Cell", group, defense: 0 };
}

describe("the symmetry vocabulary", () => {
  it("names every mode the engine can be put in", () => {
    for (const mode of SYMMETRY_MODES) {
      expect(SYMMETRY_LABELS[mode]).toBeTruthy();
    }
  });
});

describe("the palette rail", () => {
  it("files cells under their own group, in a fixed order", () => {
    const sections = paletteSections([
      cell("property", { kind: "property", property: "city", faction: undefined }),
      cell("ground", { kind: "terrain", terrain: 1 }),
      cell("water", { kind: "terrain", terrain: 28 }),
    ]);

    expect(sections.map((section) => section.drawer)).toEqual(["terrain", "property"]);
  });

  it("leaves out a drawer with nothing in it", () => {
    const sections = paletteSections([cell("ground", { kind: "terrain", terrain: 1 })]);
    expect(sections).toHaveLength(1);
  });

  it("files the eraser last in the drawer it empties", () => {
    const sections = paletteSections([
      cell("ground", { kind: "terrain", terrain: 1 }),
      cell("unit", { kind: "unit", unit: "infantry", faction: "os", hp: 10 }),
    ]);

    const land = sections.find((section) => section.drawer === "terrain");
    const units = sections.find((section) => section.drawer === "unit");
    expect(land?.cells.at(-1)?.brush).toEqual({ kind: "erase" });
    expect(units?.cells.at(-1)?.brush).toEqual({ kind: "erase-unit" });
  });

  it("keeps a drawer that holds nothing but an eraser off the rail", () => {
    const sections = paletteSections([
      cell("property", { kind: "property", property: "city", faction: undefined }),
    ]);

    expect(sections.map((section) => section.drawer)).toEqual(["property"]);
  });

  it("tells two brushes apart by what they would paint", () => {
    expect(brushKey({ kind: "terrain", terrain: 1 })).not.toBe(
      brushKey({ kind: "terrain", terrain: 2 }),
    );
    expect(brushKey({ kind: "property", property: "city", faction: "os" })).not.toBe(
      brushKey({ kind: "property", property: "city", faction: "bm" }),
    );
    expect(brushKey({ kind: "property", property: "city", faction: undefined })).toBe(
      brushKey({ kind: "property", property: "city", faction: undefined }),
    );
  });
});

describe("what the board still needs", () => {
  it("says nothing is left when every army is seated", () => {
    expect(boardNotes(state())).toEqual([
      { tone: "ready", message: "Every army is seated and can build." },
    ]);
  });

  it("asks for a headquarters on an empty board", () => {
    const notes = boardNotes(state({ armies: [] }));
    expect(notes).toHaveLength(1);
    expect(notes[0]?.message).toContain("headquarters");
  });

  it("names the army that holds no headquarters", () => {
    const notes = boardNotes(
      state({ armies: [army({ faction: "os" }), army({ faction: "bm", headquarters: 0 })] }),
    );
    expect(notes.some((note) => note.message.startsWith("Blue Moon"))).toBe(true);
  });

  it("names an army that cannot build", () => {
    const notes = boardNotes(
      state({ armies: [army({ faction: "os" }), army({ faction: "bm", production: 0 })] }),
    );
    expect(notes.some((note) => note.message.includes("cannot build"))).toBe(true);
  });

  it("reports a board that has drifted out of its own fold", () => {
    const notes = boardNotes(state({ symmetric: false }));
    expect(notes.some((note) => note.message.includes("left and right"))).toBe(true);
  });

  it("names the first tile that does not fold, and how many follow it", () => {
    const notes = boardNotes(
      state({ symmetric: false, unevenTiles: 3, firstUnevenTile: { x: 4, y: 7 } }),
    );
    expect(notes.some((note) => note.message.includes("(4, 7) and 2 more"))).toBe(true);
  });

  it("names a lone tile without a tail", () => {
    const notes = boardNotes(
      state({ symmetric: false, unevenTiles: 1, firstUnevenTile: { x: 4, y: 7 } }),
    );
    const uneven = notes.find((note) => note.message.includes("(4, 7)"));
    expect(uneven?.message).toBe("1 tile does not fold under left and right: (4, 7).");
  });

  it("keeps no opinion about armies holding different things", () => {
    // A base placed where the second army takes it on turn one is how a map
    // answers the first-turn advantage. The readout prints the counts and
    // leaves the decision alone.
    const notes = boardNotes(
      state({
        armies: [army({ faction: "os", properties: 4 }), army({ faction: "bm", properties: 6 })],
      }),
    );

    expect(notes).toEqual([{ tone: "ready", message: "Every army is seated and can build." }]);
  });

  it("says nothing about symmetry when no fold is set", () => {
    const notes = boardNotes(state({ symmetry: "none", symmetric: false }));
    expect(notes.every((note) => !note.message.includes("read the same"))).toBe(true);
  });
});

describe("what saving does", () => {
  it("says which map the button writes to", () => {
    expect(saveLabel("create")).toBe("Keep this map");
    expect(saveLabel("revise")).toContain("revision");
    expect(saveLabel("fork")).toContain("my own");
  });

  it("names the revision an edit will become", () => {
    expect(saveBlurb("revise", 3)).toContain("Revision 4");
  });
});

describe("stepping along a drawer", () => {
  const cells = [
    cell("ground", { kind: "terrain", terrain: 1 }),
    cell("ground", { kind: "terrain", terrain: 2 }),
    cell("ground", { kind: "terrain", terrain: 3 }),
  ];

  it("moves to the next key and to the one before", () => {
    expect(stepBrush(cells, { kind: "terrain", terrain: 2 }, 1)).toEqual({
      kind: "terrain",
      terrain: 3,
    });
    expect(stepBrush(cells, { kind: "terrain", terrain: 2 }, -1)).toEqual({
      kind: "terrain",
      terrain: 1,
    });
  });

  it("wraps at both ends, so the last key is never the hardest to reach", () => {
    expect(stepBrush(cells, { kind: "terrain", terrain: 3 }, 1)).toEqual({
      kind: "terrain",
      terrain: 1,
    });
    expect(stepBrush(cells, { kind: "terrain", terrain: 1 }, -1)).toEqual({
      kind: "terrain",
      terrain: 3,
    });
  });

  it("starts at the front when the loaded brush is not in this drawer", () => {
    expect(stepBrush(cells, { kind: "erase-unit" }, 1)).toEqual({ kind: "terrain", terrain: 1 });
    expect(stepBrush(cells, null, 1)).toEqual({ kind: "terrain", terrain: 1 });
  });

  it("answers nothing for an empty drawer", () => {
    expect(stepBrush([], { kind: "terrain", terrain: 1 }, 1)).toBeNull();
  });

  it("empties the ground in a drawer of ground and the unit in a drawer of units", () => {
    expect(eraserOfDrawer("terrain")).toEqual({ kind: "erase" });
    expect(eraserOfDrawer("property")).toEqual({ kind: "erase" });
    expect(eraserOfDrawer("unit")).toEqual({ kind: "erase-unit" });
  });
});

describe("a key's name", () => {
  it("marks where a name too long for one line may break", () => {
    expect(keyName("Submarine")).toBe("Sub\u00ADmarine");
    expect(keyName("Piperunner")).toBe("Pipe\u00ADrunner");
  });

  it("leaves a name that fits alone", () => {
    expect(keyName("Plain")).toBe("Plain");
    expect(keyName("Missile silo")).toBe("Missile silo");
  });
});

describe("the key legend", () => {
  it("gives every row a key and something it does", () => {
    for (const shortcut of EDITOR_SHORTCUTS) {
      expect(shortcut.keys.length).toBeGreaterThan(0);
      expect(shortcut.does).toBeTruthy();
    }
  });

  it("prints a fragment against each key rather than a sentence", () => {
    for (const shortcut of EDITOR_SHORTCUTS) {
      expect(shortcut.does.length).toBeLessThanOrEqual(24);
      expect(shortcut.does).not.toMatch(/\.$/);
    }
  });

  it("leaves the pointer gestures to the strip under the board", () => {
    for (const shortcut of EDITOR_SHORTCUTS) {
      expect(shortcut.keys.join(" ")).not.toMatch(/alt/i);
      expect(shortcut.does).not.toMatch(/click/i);
    }
  });
});

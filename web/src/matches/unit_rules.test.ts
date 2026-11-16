import { describe, expect, it } from "vitest";
import beachNation from "../../../assets/maps/73021.json";
import normandie from "../../../assets/maps/67073.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { importAwbwMapDocument, initSync, WasmMatch } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";
import { defaultMatchClock, matchSettingsSchema } from "./schemas.ts";
import {
  labTiles,
  setUnitState,
  unitCatalog,
  unitRulesForMap,
  type UnitRules,
} from "./unit_rules.ts";

initSync({ module: serverWasmModule });

const settings = { fogEnabled: false, startingFunds: 0, clock: defaultMatchClock };

describe("unit settings", () => {
  it("stores each list once, in the build list's order", () => {
    const parsed = matchSettingsSchema.parse({
      ...settings,
      unitBans: ["stealth", "piperunner", "stealth"],
      labUnits: ["carrier", "neo-tank"],
    });
    expect(parsed.unitBans).toEqual(["piperunner", "stealth"]);
    expect(parsed.labUnits).toEqual(["neo-tank", "carrier"]);
  });

  it("refuses a kind the ruleset does not have", () => {
    expect(matchSettingsSchema.safeParse({ ...settings, unitBans: ["zeppelin"] }).success).toBe(
      false,
    );
  });

  it("refuses a kind that is both banned and a Lab unit", () => {
    expect(
      matchSettingsSchema.safeParse({ ...settings, unitBans: ["tank"], labUnits: ["tank"] })
        .success,
    ).toBe(false);
  });

  it("allows every kind banned", () => {
    const every = unitCatalog.map((entry) => entry.kind);
    expect(matchSettingsSchema.parse({ ...settings, unitBans: every }).unitBans).toEqual(every);
  });

  it("moves a kind between the lists rather than keeping it in both", () => {
    const banned = setUnitState({ unitBans: [], labUnits: ["tank"] }, ["tank"], "banned");
    expect(banned).toEqual({ unitBans: ["tank"], labUnits: [] });
  });

  it("bans the Lab units on a map with no Lab, and keeps them on one with a Lab", () => {
    const rules: UnitRules = { unitBans: ["stealth"], labUnits: ["neo-tank", "bomber"] };
    expect(unitRulesForMap(rules, false)).toEqual({
      unitBans: ["neo-tank", "bomber", "stealth"],
      labUnits: [],
    });
    expect(unitRulesForMap(rules, true)).toBe(rules);
  });

  it("finds a map's Labs by tile, row by row", () => {
    const { document } = importAwbwMapDocument(awbwMapDataSchema.parse(beachNation));
    // Red Fire's Lab at (9, 13) and Brown Desert's at (13, 5), on a 23-wide map.
    expect(labTiles(document.terrain)).toEqual([5 * 23 + 13, 13 * 23 + 9]);
  });
});

describe("unit settings in the engine", () => {
  const { document } = importAwbwMapDocument(awbwMapDataSchema.parse(normandie));
  const start = (rules: { unitBans?: ("infantry" | "mech")[]; labUnits?: "mech"[] }) =>
    new WasmMatch({
      map: document,
      players: [
        { factionId: 4, team: null, startingFunds: 10_000, coId: 1 },
        { factionId: 8, team: null, startingFunds: 10_000, coId: 1 },
      ],
      fogEnabled: false,
      startingFunds: 10_000,
      rngSeed: 1,
      ...rules,
    });
  // Yellow Comet's base in Normandie's corner.
  const build = (match: WasmMatch, unit: "infantry" | "mech") => () =>
    match.process_action(0, { type: "build", position: { x: 0, y: 2 }, unit_type: unit });

  it("refuses a banned kind and a Lab unit without a Lab", () => {
    const match = start({ unitBans: ["infantry"], labUnits: ["mech"] });
    expect(build(match, "infantry")).toThrow();
    expect(build(match, "mech")).toThrow();
  });

  it("builds a kind the match leaves alone", () => {
    expect(build(start({}), "infantry")).not.toThrow();
  });
});

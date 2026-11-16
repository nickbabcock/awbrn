/**
 * Unit bans and Lab units: two match-wide lists of unit kinds.
 *
 * A banned kind is built by nobody. A Lab unit is built only by a player who
 * owns a Lab of their own. The engine rules on both
 * (`spec/semantics/production.md`); this module only says what the lists are.
 */

import type { UnitKind } from "#/wasm/awbrn_wasm.js";

export type UnitGroup = "ground" | "air" | "sea";

export interface UnitEntry {
  kind: UnitKind;
  name: string;
  cost: number;
  group: UnitGroup;
}

/** Every kind, grouped as the facilities build them and in cost order: the build list's order. */
export const unitCatalog: readonly UnitEntry[] = [
  { kind: "infantry", name: "Infantry", cost: 1000, group: "ground" },
  { kind: "mech", name: "Mech", cost: 3000, group: "ground" },
  { kind: "recon", name: "Recon", cost: 4000, group: "ground" },
  { kind: "apc", name: "APC", cost: 5000, group: "ground" },
  { kind: "artillery", name: "Artillery", cost: 6000, group: "ground" },
  { kind: "tank", name: "Tank", cost: 7000, group: "ground" },
  { kind: "anti-air", name: "Anti-Air", cost: 8000, group: "ground" },
  { kind: "missile", name: "Missile", cost: 12000, group: "ground" },
  { kind: "rocket", name: "Rocket", cost: 15000, group: "ground" },
  { kind: "md-tank", name: "MD Tank", cost: 16000, group: "ground" },
  { kind: "piperunner", name: "Piperunner", cost: 20000, group: "ground" },
  { kind: "neo-tank", name: "Neo Tank", cost: 22000, group: "ground" },
  { kind: "mega-tank", name: "Mega Tank", cost: 28000, group: "ground" },
  { kind: "t-copter", name: "T-Copter", cost: 5000, group: "air" },
  { kind: "b-copter", name: "B-Copter", cost: 9000, group: "air" },
  { kind: "fighter", name: "Fighter", cost: 20000, group: "air" },
  { kind: "bomber", name: "Bomber", cost: 22000, group: "air" },
  { kind: "stealth", name: "Stealth", cost: 24000, group: "air" },
  { kind: "black-bomb", name: "Black Bomb", cost: 25000, group: "air" },
  { kind: "black-boat", name: "Black Boat", cost: 7500, group: "sea" },
  { kind: "lander", name: "Lander", cost: 12000, group: "sea" },
  { kind: "cruiser", name: "Cruiser", cost: 18000, group: "sea" },
  { kind: "sub", name: "Submarine", cost: 20000, group: "sea" },
  { kind: "battleship", name: "Battleship", cost: 28000, group: "sea" },
  { kind: "carrier", name: "Carrier", cost: 30000, group: "sea" },
];

export const unitGroups: readonly { group: UnitGroup; name: string; facility: string }[] = [
  { group: "ground", name: "Ground", facility: "Base" },
  { group: "air", name: "Air", facility: "Airport" },
  { group: "sea", name: "Sea", facility: "Port" },
];

const catalogOrder = new Map(unitCatalog.map((entry, index) => [entry.kind, index]));

export function isUnitKind(value: unknown): value is UnitKind {
  return catalogOrder.has(value as UnitKind);
}

export function unitName(kind: UnitKind): string {
  return unitCatalog[catalogOrder.get(kind)!]!.name;
}

/** A list as a match stores it: known kinds, once each, in catalog order. */
export function unitList(kinds: Iterable<string>): UnitKind[] {
  return [...new Set(kinds)]
    .filter(isUnitKind)
    .sort((left, right) => catalogOrder.get(left)! - catalogOrder.get(right)!);
}

/** The two lists. A kind is in at most one of them; in neither, anyone may build it. */
export interface UnitRules {
  unitBans: readonly UnitKind[];
  labUnits: readonly UnitKind[];
}

export const NO_UNIT_RULES: UnitRules = { unitBans: [], labUnits: [] };

export type UnitState = "buildable" | "banned" | "lab";

export function unitState(rules: UnitRules, kind: UnitKind): UnitState {
  if (rules.unitBans.includes(kind)) return "banned";
  if (rules.labUnits.includes(kind)) return "lab";
  return "buildable";
}

/** Put every one of `kinds` in `state`, taking each out of the other list. */
export function setUnitState(
  rules: UnitRules,
  kinds: readonly UnitKind[],
  state: UnitState,
): UnitRules {
  const moving = new Set(kinds);
  const keep = (list: readonly UnitKind[]) => list.filter((kind) => !moving.has(kind));
  return {
    unitBans: unitList([...keep(rules.unitBans), ...(state === "banned" ? kinds : [])]),
    labUnits: unitList([...keep(rules.labUnits), ...(state === "lab" ? kinds : [])]),
  };
}

/**
 * The rules on a map with or without a Lab. Where no Lab stands, a Lab unit
 * can never be built, so it is banned, which says the same thing where the
 * host can see it.
 */
export function unitRulesForMap(rules: UnitRules, hasLab: boolean): UnitRules {
  return hasLab || rules.labUnits.length === 0
    ? rules
    : { unitBans: unitList([...rules.unitBans, ...rules.labUnits]), labUnits: [] };
}

/** The AWBW terrain ids of a Lab, neutral or held by any army. */
const LAB_TERRAIN_IDS: ReadonlySet<number> = new Set([
  138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 154, 161, 168, 175, 186, 193, 201, 208,
  215, 222,
]);

/** Where the map's Labs stand, as tile indexes into its terrain. */
export function labTiles(terrain: readonly number[]): number[] {
  return terrain.flatMap((id, index) => (LAB_TERRAIN_IDS.has(id) ? [index] : []));
}

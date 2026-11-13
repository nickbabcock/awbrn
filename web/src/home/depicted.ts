/**
 * The pictures the landing page draws of the product: which maps it shows, and
 * where a depicted skirmish puts its units on a real map's terrain.
 *
 * A depicted screen is an inert picture of a match, not a match. Its board is
 * the map's own render; its units are placed here, on tiles a land unit can
 * stand on, so any map the catalog ranks reads as a game in progress.
 */

import type { UnitKind } from "#/wasm/awbrn_wasm.js";
import type { MapCatalogEntry, MapRank } from "#/maps/schemas.ts";

/** How strongly a rank recommends a map: S first. */
const RANK_ORDER: Record<MapRank, number> = { S: 0, A: 1, B: 2, C: 3 };

/** The size a first match is best played on: about 20 by 20. */
const FIRST_MATCH_AREA = 20 * 20;

/** Ranked maps, strongest rank first, keeping the catalog's order inside a rank. */
export function byRank(maps: readonly MapCatalogEntry[]): MapCatalogEntry[] {
  return maps
    .filter((map) => map.rank !== null)
    .map((map, index) => ({ map, index }))
    .sort(
      (left, right) =>
        RANK_ORDER[left.map.rank!] - RANK_ORDER[right.map.rank!] || left.index - right.index,
    )
    .map(({ map }) => map);
}

/**
 * The map a first match is played on: an S-rank map for two armies, the one
 * nearest 20 by 20. Falls back to the strongest two-army map. A first match
 * seats the player and one CPU, so a map for more armies is never chosen and
 * a catalog with no ranked duel gives null.
 */
export function chooseFirstMatchMap(maps: readonly MapCatalogEntry[]): MapCatalogEntry | null {
  const ranked = byRank(maps);
  const duels = ranked.filter((map) => map.playerCount === 2);
  const best = duels.filter((map) => map.rank === "S");
  const pool = best.length > 0 ? best : duels;
  let chosen: MapCatalogEntry | null = null;
  for (const map of pool) {
    if (
      chosen === null ||
      Math.abs(map.width * map.height - FIRST_MATCH_AREA) <
        Math.abs(chosen.width * chosen.height - FIRST_MATCH_AREA)
    ) {
      chosen = map;
    }
  }
  return chosen;
}

/** Map rows beside the landing's featured map: four on desktop. */
export const LANDING_MAP_ROWS = 4;

/**
 * The landing's maps, from the ranked list: the featured map (the one a first
 * match is played on; a catalog with no ranked duel still features its
 * strongest map), the map rows beside it, and the map the depicted replay
 * shows (another two-army map, so the page is not one map twice). The route
 * loads the featured and replay maps' terrain with the page, so the depicted
 * boards are drawn with their units at first paint.
 */
export function chooseLandingMaps(maps: readonly MapCatalogEntry[]): {
  featured: MapCatalogEntry | null;
  rows: MapCatalogEntry[];
  replay: MapCatalogEntry | null;
} {
  const ranked = byRank(maps);
  const featured = chooseFirstMatchMap(ranked) ?? ranked[0] ?? null;
  const rows = ranked.filter((map) => map !== featured).slice(0, LANDING_MAP_ROWS);
  const replay = rows.find((map) => map.playerCount === 2) ?? featured;
  return { featured, rows, replay };
}

/** A unit on a depicted board, at a tile. */
export interface DepictedUnit {
  x: number;
  y: number;
  unit: UnitKind;
  /** 0 for the first army (the viewer's), 1 for the other. */
  side: 0 | 1;
  /** Shown only below 10, as the game does. */
  hp: number;
}

/**
 * A skirmish around the board's middle, as offsets from the selected unit:
 * the viewer's army to the left and below, the other army to the right and
 * above. The first entry is the unit the depicted cursor sits on.
 */
const SKIRMISH: readonly (Omit<DepictedUnit, "x" | "y"> & { dx: number; dy: number })[] = [
  { dx: 0, dy: 0, unit: "tank", side: 0, hp: 10 },
  { dx: -2, dy: 0, unit: "infantry", side: 0, hp: 10 },
  { dx: -4, dy: 0, unit: "mech", side: 0, hp: 10 },
  { dx: -5, dy: 1, unit: "artillery", side: 0, hp: 10 },
  { dx: -1, dy: 1, unit: "infantry", side: 0, hp: 6 },
  { dx: -7, dy: 1, unit: "recon", side: 0, hp: 10 },
  { dx: -3, dy: 3, unit: "anti-air", side: 0, hp: 10 },
  { dx: -6, dy: 2, unit: "md-tank", side: 0, hp: 8 },
  { dx: -8, dy: -1, unit: "infantry", side: 0, hp: 10 },
  { dx: -9, dy: 3, unit: "apc", side: 0, hp: 10 },
  { dx: -6, dy: 5, unit: "infantry", side: 0, hp: 10 },
  { dx: 1, dy: 0, unit: "recon", side: 1, hp: 10 },
  { dx: 2, dy: -2, unit: "md-tank", side: 1, hp: 7 },
  { dx: 0, dy: -3, unit: "infantry", side: 1, hp: 10 },
  { dx: 4, dy: -3, unit: "artillery", side: 1, hp: 10 },
  { dx: 3, dy: -1, unit: "tank", side: 1, hp: 10 },
  { dx: 5, dy: -1, unit: "anti-air", side: 1, hp: 10 },
  { dx: 3, dy: 0, unit: "infantry", side: 1, hp: 4 },
  { dx: -1, dy: -3, unit: "mech", side: 1, hp: 10 },
  { dx: 6, dy: -2, unit: "apc", side: 1, hp: 10 },
  { dx: 1, dy: -4, unit: "infantry", side: 1, hp: 10 },
];

/**
 * AWBW terrain a land unit stands on in the open: plain, wood, road and
 * bridge. Properties, mountains, rivers, sea and pipes are left clear, so the
 * picture never shows a tank on a reef or a unit hiding a city.
 */
function isOpenGround(terrain: number): boolean {
  return terrain === 1 || terrain === 3 || (terrain >= 15 && terrain <= 27);
}

/** The tile the depicted cursor sits on, and the units around it. */
export interface DepictedSkirmish {
  focus: { x: number; y: number };
  /** The terrain stars of the tile under the cursor. */
  focusDefense: number;
  units: DepictedUnit[];
}

/** Terrain stars of the open ground a skirmish stands on: wood 2, plain 1, road 0. */
function openGroundDefense(terrain: number): number {
  if (terrain === 3) return 2;
  if (terrain === 1) return 1;
  return 0;
}

/**
 * Lays the skirmish onto a map: each unit takes the open tile nearest its
 * place in the formation that nothing else holds, and is left out when none
 * lies within two tiles. The formation centers on the open tile nearest the
 * map's middle.
 */
export function placeSkirmish(document: {
  width: number;
  height: number;
  terrain: readonly number[];
  units: readonly { position: readonly [number, number] }[];
}): DepictedSkirmish {
  const { width, height, terrain } = document;
  const taken = new Set(document.units.map(({ position: [x, y] }) => y * width + x));
  const isFree = (x: number, y: number) =>
    x >= 0 &&
    y >= 0 &&
    x < width &&
    y < height &&
    !taken.has(y * width + x) &&
    isOpenGround(terrain[y * width + x] ?? 0);
  const nearestFree = (x: number, y: number, reach: number) => {
    for (let ring = 0; ring <= reach; ring += 1) {
      for (let dy = -ring; dy <= ring; dy += 1) {
        for (let dx = -ring; dx <= ring; dx += 1) {
          if (Math.max(Math.abs(dx), Math.abs(dy)) !== ring) continue;
          if (isFree(x + dx, y + dy)) return { x: x + dx, y: y + dy };
        }
      }
    }
    return null;
  };

  const middle = { x: Math.floor(width / 2), y: Math.floor(height / 2) };
  const focus = nearestFree(middle.x, middle.y, Math.max(width, height)) ?? middle;
  const units: DepictedUnit[] = [];
  for (const { dx, dy, ...unit } of SKIRMISH) {
    const isFocus = dx === 0 && dy === 0;
    const spot = isFocus ? focus : nearestFree(focus.x + dx, focus.y + dy, 2);
    if (spot === null) continue;
    taken.add(spot.y * width + spot.x);
    units.push({ ...unit, ...spot });
  }
  return {
    focus,
    focusDefense: openGroundDefense(terrain[focus.y * width + focus.x] ?? 1),
    units,
  };
}

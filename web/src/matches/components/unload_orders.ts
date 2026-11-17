import type { CargoBadge, DropAim, UnitActionOption } from "#/wasm/awbrn_wasm.js";

/** An order that puts cargo down, with the cargo it names. */
export type UnloadOption = UnitActionOption & {
  action: Extract<UnitActionOption["action"], { type: "unload" }>;
  cargo: CargoBadge;
};

export function isUnload(option: UnitActionOption | undefined): option is UnloadOption {
  return option?.action.type === "unload" && option.cargo !== undefined;
}

/**
 * How far a menu about a tile opens from its anchor, clear of the whole tile.
 *
 * Cargo lands beside the transport, and a forecast is most often about the
 * enemy beside it, so a menu with Unload or forecast rows clears one tile more
 * and does not cover the neighbor it is about.
 */
export function menuClearance(tilePx: number, options: UnitActionOption[] = []): number {
  const tile = tilePx / 2 + 8;
  return options.some((option) => isUnload(option) || option.forecast) ? tile + tilePx : tile;
}

/**
 * What an Unload row is called aloud. The row draws no tile, so the count of
 * places the cargo may land is what tells a listener there is a choice to come.
 * The health and marks the sprite wears are what tell two of a kind apart.
 */
export function unloadLabel(option: UnloadOption): string {
  const { cargo } = option;
  const count = option.action.tiles.length;
  const health = cargo.health !== undefined && cargo.health < 10 ? ` at ${cargo.health} HP` : "";
  const marks = `${cargo.lowFuel ? ", low fuel" : ""}${cargo.lowAmmo ? ", low ammo" : ""}`;
  return `Unload ${cargo.name}${health}${marks}, ${count} ${count === 1 ? "tile" : "tiles"}`;
}

/** What the live region says once a tile is aimed: "Infantry to 12, 9, Plain, 1 star". */
export function aimAnnouncement(cargo: CargoBadge, aim: DropAim): string {
  const stars = `${aim.defenseStars} ${aim.defenseStars === 1 ? "star" : "stars"}`;
  return `${cargo.name} to ${aim.x}, ${aim.y}, ${aim.terrainName}, ${stars}`;
}

/**
 * The phone dock's rows, as indexes into the orders.
 *
 * With cargo aboard, every Unload key shares the first row and the other
 * orders share the second, so the dock holds the height of the tile row and
 * dock it stands over. A Fire row carries its forecast and keeps a row of its
 * own, as every order does without cargo.
 */
export function dockRows(options: UnitActionOption[]): number[][] {
  const indexes = options.map((_, index) => index);
  const unloads = indexes.filter((index) => isUnload(options[index]));
  if (unloads.length === 0) return indexes.map((index) => [index]);
  const rest = indexes.filter((index) => !isUnload(options[index]));
  const fire = rest.filter((index) => options[index]!.forecast).map((index) => [index]);
  const keys = rest.filter((index) => !options[index]!.forecast);
  // Three keys share a 360px row without cutting a name short; a rare fourth
  // grows the dock rather than shrinking every key.
  const shared = [];
  for (let start = 0; start < keys.length; start += 3) shared.push(keys.slice(start, start + 3));
  return [unloads, ...fire, ...shared];
}

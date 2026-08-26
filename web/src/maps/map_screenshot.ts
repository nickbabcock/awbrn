/**
 * Where a map's pictures live in the content bucket.
 *
 * Every map has two of them: the full board at sprite size, and the small
 * picture a listing shows. They are keyed by the same content hash the map
 * document is keyed by, so two maps with identical content share one picture
 * and a picture never goes stale against the board it draws.
 */

/** The pictures a map has. */
export const MAP_SCREENSHOT_KINDS = ["full", "small"] as const;

export type MapScreenshotKind = (typeof MAP_SCREENSHOT_KINDS)[number];

/** A picture is immutable: its key names the content it was drawn from. */
export const MAP_SCREENSHOT_CACHE_CONTROL = "public, max-age=31536000, immutable";

export const MAP_SCREENSHOT_CONTENT_TYPE = "image/png";

export function mapScreenshotKey(contentHash: string, kind: MapScreenshotKind): string {
  return `maps/img/v1/${contentHash}/${kind}.png`;
}

/**
 * Where the browser asks for a picture.
 *
 * The address names the content and not the map, so a picture can be cached
 * for a year: an edit to the map makes a new hash and therefore a new address.
 */
export function mapScreenshotPath(contentHash: string, kind: MapScreenshotKind): string {
  return `/api/maps/img/${contentHash}/${kind}.png`;
}

/** How many pixels one map tile takes in each picture. */
export const MAP_SCREENSHOT_TILE_SIZE: Record<MapScreenshotKind, number> = {
  full: 16,
  small: 4,
};

/**
 * The size of a picture, from the size of the map it draws.
 *
 * The full picture carries the terrain overhang as one more row above the
 * board; the small picture has no overhang. `map_screenshot.test.ts` holds the
 * renderer to both.
 */
export function mapScreenshotSize(
  kind: MapScreenshotKind,
  width: number,
  height: number,
): { width: number; height: number } {
  const tile = MAP_SCREENSHOT_TILE_SIZE[kind];
  return { width: width * tile, height: (kind === "full" ? height + 1 : height) * tile };
}

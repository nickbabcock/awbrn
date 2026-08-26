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

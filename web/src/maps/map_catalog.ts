/**
 * How the map catalog is paged and searched.
 *
 * The catalog is the list of maps AWBRN holds. A map enters it by import and
 * stays, so the catalog only grows and a page can be addressed by the position
 * of its last row. Newest first, because the map a player just imported is the
 * map they came to play.
 */

import { z } from "zod";
import { decodeCursor } from "#/utils/cursor.ts";
import { MAP_ID_LENGTH } from "./map_id.ts";

export const MAP_CATALOG_PAGE_SIZE = 24;

/** The longest search text the catalog reads. Longer text is cut. */
export const MAP_SEARCH_MAX_LENGTH = 80;

const mapCatalogCursorSchema = z.object({
  createdAt: z.iso.datetime(),
  mapId: z
    .string()
    .length(MAP_ID_LENGTH)
    .regex(/^[0-9a-z]+$/),
});

export type MapCatalogCursor = z.infer<typeof mapCatalogCursorSchema>;

export function encodeMapCatalogCursor(cursor: MapCatalogCursor): string {
  return JSON.stringify(cursor);
}

export function decodeMapCatalogCursor(cursor: string | undefined): MapCatalogCursor | null {
  return decodeCursor(cursor, mapCatalogCursorSchema);
}

/**
 * Read the player's search text.
 *
 * Returns null when nothing is left to search for, which is what tells the
 * catalog to list everything instead of matching against an empty string.
 */
export function normalizeMapSearch(search: string | null | undefined): string | null {
  if (!search) return null;
  const collapsed = search.trim().replace(/\s+/g, " ").slice(0, MAP_SEARCH_MAX_LENGTH);
  return collapsed.length > 0 ? collapsed : null;
}

/**
 * Build the pattern for a case-insensitive contains match.
 *
 * The wildcards of `LIKE` are escaped so a map named "100%" is searched for by
 * name and does not match every map in the catalog.
 */
export function mapSearchPattern(search: string): string {
  return `%${search.replace(/[\\%_]/g, (character) => `\\${character}`)}%`;
}

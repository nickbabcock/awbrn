/**
 * The catalog board, written into its own address.
 *
 * A board somebody narrowed is worth sending to somebody else, so the search
 * text and the three filter rows live in the query string rather than in the
 * screen. The address is written for a person to read: each row is one
 * parameter holding its pressed keys, separated by commas, and a row nothing
 * is pressed on is left out.
 *
 * Everything read out of an address is checked against the vocabulary it
 * belongs to and anything else is dropped, so a hand-edited address narrows
 * the board or does nothing, and never breaks it.
 */

import { MAP_SEARCH_MAX_LENGTH } from "./map_catalog.ts";
import { MAP_RANK_FILTERS } from "./map_taxonomy.ts";
import { MAP_PLAYER_COUNT_FILTERS, MAP_TAGS, type MapCatalogFilter } from "./schemas.ts";

/** The board's address, as the route validates and writes it. */
export interface MapBoardSearch {
  q?: string;
  armies?: string;
  tags?: string;
  rank?: string;
}

export function validateMapBoardSearch(search: Record<string, unknown>): MapBoardSearch {
  return {
    ...text("q", search.q),
    ...list("armies", MAP_PLAYER_COUNT_FILTERS, search.armies),
    ...list("tags", MAP_TAGS, search.tags),
    ...list("rank", MAP_RANK_FILTERS, search.rank),
  };
}

/** The search text of an address, which is empty until somebody types. */
export function mapBoardSearchText(search: MapBoardSearch): string {
  return search.q ?? "";
}

/** The three filter rows of an address, each in vocabulary order. */
export function mapBoardFilters(search: MapBoardSearch): Required<MapCatalogFilter> {
  return {
    playerCounts: parse(MAP_PLAYER_COUNT_FILTERS, search.armies),
    tags: parse(MAP_TAGS, search.tags),
    ranks: parse(MAP_RANK_FILTERS, search.rank),
  };
}

/**
 * The address a board holds, with every empty part left out.
 *
 * An empty part is left out rather than written as an empty value, so the
 * board at rest is addressed by `/maps` and nothing else.
 */
export function mapBoardAddress(text: string, filters: Required<MapCatalogFilter>): MapBoardSearch {
  const trimmed = text.trim().slice(0, MAP_SEARCH_MAX_LENGTH);
  return {
    ...(trimmed ? { q: trimmed } : {}),
    ...(filters.playerCounts.length > 0 ? { armies: filters.playerCounts.join(",") } : {}),
    ...(filters.tags.length > 0 ? { tags: filters.tags.join(",") } : {}),
    ...(filters.ranks.length > 0 ? { rank: filters.ranks.join(",") } : {}),
  };
}

function text(key: "q", value: unknown): Partial<MapBoardSearch> {
  if (typeof value !== "string") return {};
  const trimmed = value.trim().slice(0, MAP_SEARCH_MAX_LENGTH);
  return trimmed ? { [key]: trimmed } : {};
}

function list<T extends string>(
  key: keyof MapBoardSearch,
  vocabulary: readonly T[],
  value: unknown,
): Partial<MapBoardSearch> {
  const held = parse(vocabulary, value);
  return held.length > 0 ? { [key]: held.join(",") } : {};
}

/** The values of one row, each one once and in the vocabulary's own order. */
function parse<T extends string>(vocabulary: readonly T[], value: unknown): T[] {
  if (typeof value !== "string" || value.length === 0) return [];
  const wanted = new Set(value.split(","));
  return vocabulary.filter((option) => wanted.has(option)) as T[];
}

/**
 * How ranks and tags are ordered and compared.
 *
 * The vocabularies live in `schemas.ts`; what is here is the ordering the
 * catalog and the screens read them in. Both vocabularies are written in
 * their own order — ranks from worst to best, tags in the order they are
 * shown — so a position in the vocabulary is the whole ordering.
 */

import { MAP_PLAYER_COUNT_FILTERS, MAP_RANKS, MAP_TAGS, MAP_UNRANKED_FILTER } from "./schemas.ts";
import type { MapCatalogFilter, MapRank, MapRankFilter, MapTag } from "./schemas.ts";

/**
 * The rank buttons on the board, best first, with the unranked maps last.
 *
 * `MAP_RANKS` reads worst to best because that is the order a rank is earned
 * in; a row of buttons reads the other way, so it is reversed once here.
 */
export const MAP_RANK_FILTERS: readonly MapRankFilter[] = [
  ...[...MAP_RANKS].reverse(),
  MAP_UNRANKED_FILTER,
];

/** Where a rank stands, counting from 0 at C. */
export function mapRankOrder(rank: MapRank): number {
  return MAP_RANKS.indexOf(rank);
}

/**
 * Compare two ranks, best first. An unranked revision sorts last.
 *
 * Use it as the comparator of a sort, the way the catalog lists its best
 * maps before the rest.
 */
export function compareMapRanks(left: MapRank | null, right: MapRank | null): number {
  if (left === right) return 0;
  if (left === null) return 1;
  if (right === null) return -1;
  return mapRankOrder(right) - mapRankOrder(left);
}

/** True while `rank` is at least as good as `least`. */
export function mapRankAtLeast(rank: MapRank | null, least: MapRank): boolean {
  return rank !== null && mapRankOrder(rank) >= mapRankOrder(least);
}

/**
 * The tags of a map, each one once and in vocabulary order.
 *
 * Tags reach us as a set the player picked and leave the database in the
 * order the rows happened to be read, so both ends are put in one order here.
 */
export function sortMapTags(tags: readonly MapTag[]): MapTag[] {
  const wanted = new Set(tags);
  return MAP_TAGS.filter((tag) => wanted.has(tag));
}

/**
 * One filter set, written the same way every time.
 *
 * Every list is put in its vocabulary order with each value once, and a list
 * that names every value it could name is dropped: filtering by all four
 * player counts is the same board as filtering by none, and writing it as
 * none keeps one board under one query key instead of two.
 */
export function normalizeMapCatalogFilters(
  filters: MapCatalogFilter | null | undefined,
): Required<MapCatalogFilter> {
  return {
    playerCounts: narrowing(MAP_PLAYER_COUNT_FILTERS, filters?.playerCounts),
    ranks: narrowing(MAP_RANK_FILTERS, filters?.ranks),
    tags: narrowing(MAP_TAGS, filters?.tags),
  };
}

/** True while `filters` leaves the board as wide as it was. */
export function isMapCatalogFilterEmpty(filters: Required<MapCatalogFilter>): boolean {
  return (
    filters.playerCounts.length === 0 && filters.ranks.length === 0 && filters.tags.length === 0
  );
}

/** How many of the board's filter buttons are pressed. */
export function countMapCatalogFilters(filters: Required<MapCatalogFilter>): number {
  return filters.playerCounts.length + filters.ranks.length + filters.tags.length;
}

/**
 * The chosen values in vocabulary order, or nothing when they are all chosen.
 *
 * A filter that selects everything selects nothing, which is what keeps a
 * full row of pressed buttons from being written into the query.
 */
function narrowing<T extends string>(vocabulary: readonly T[], chosen: readonly T[] | undefined) {
  if (!chosen || chosen.length === 0) return [];
  const wanted = new Set(chosen);
  const ordered = vocabulary.filter((value) => wanted.has(value));
  return ordered.length === vocabulary.length ? [] : ordered;
}

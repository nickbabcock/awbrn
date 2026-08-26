/**
 * How ranks and tags are ordered and compared.
 *
 * The vocabularies live in `schemas.ts`; what is here is the ordering the
 * catalog and the screens read them in. Both vocabularies are written in
 * their own order — ranks from worst to best, tags in the order they are
 * shown — so a position in the vocabulary is the whole ordering.
 */

import { MAP_RANKS, MAP_TAGS } from "./schemas.ts";
import type { MapRank, MapTag } from "./schemas.ts";

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

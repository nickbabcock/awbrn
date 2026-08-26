/**
 * What a narrowed map board asks of the database.
 *
 * The conditions live apart from the rest of the catalog reads because they
 * are the only part of the statement that changes with what a player pressed,
 * and because a predicate with no binding to a live database can be run
 * against a scratch one and checked.
 */

import { eq, gte, inArray, isNull, or, sql, type SQL } from "drizzle-orm";
import { mapRevisions, maps, mapTags } from "#/db/global.ts";
import { MAP_LARGE_PLAYER_COUNT, MAP_UNRANKED_FILTER } from "./schemas.ts";
import type { MapCatalogFilter, MapRank } from "./schemas.ts";

/**
 * One condition per question the player asked, and none for the rest.
 *
 * A filter that names nothing adds no condition, so an unfiltered board reads
 * the same statement it always did.
 */
export function catalogFilterConditions(filters: Required<MapCatalogFilter>): SQL[] {
  const conditions: SQL[] = [];

  if (filters.playerCounts.length > 0) {
    const seats = or(
      ...filters.playerCounts.map((count) =>
        count === "5+"
          ? gte(mapRevisions.playerCount, MAP_LARGE_PLAYER_COUNT)
          : eq(mapRevisions.playerCount, Number(count)),
      ),
    );
    if (seats) conditions.push(seats);
  }

  if (filters.ranks.length > 0) {
    const ranked = filters.ranks.filter((rank): rank is MapRank => rank !== MAP_UNRANKED_FILTER);
    const rank = or(
      ...(ranked.length > 0 ? [inArray(mapRevisions.rank, ranked)] : []),
      ...(filters.ranks.length > ranked.length ? [isNull(mapRevisions.rank)] : []),
    );
    if (rank) conditions.push(rank);
  }

  if (filters.tags.length > 0) {
    // Every named tag has to be on the map, so the rows are counted rather
    // than matched: a map carrying two of three named tags is not this board.
    conditions.push(
      sql`${maps.id} in (
        select ${mapTags.mapId} from ${mapTags}
        where ${inArray(mapTags.tag, [...filters.tags])}
        group by ${mapTags.mapId}
        having count(distinct ${mapTags.tag}) = ${filters.tags.length}
      )`,
    );
  }

  return conditions;
}

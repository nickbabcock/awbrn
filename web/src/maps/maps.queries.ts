import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { normalizeMapSearch } from "./map_catalog.ts";
import { normalizeMapCatalogFilters } from "./map_taxonomy.ts";
import { getMapCatalogEntryFn, getMapRevisionFn, listMapsFn } from "./maps.functions.ts";
import { mapKeys } from "./maps.keys.ts";
import type { MapCatalogFilter } from "./schemas.ts";

export function mapRevisionQueryOptions(mapId: string, revision: number) {
  return queryOptions({
    queryKey: mapKeys.revision(mapId, revision),
    queryFn: () => getMapRevisionFn({ data: { mapId, revision } }),
    staleTime: Infinity,
  });
}

/** A map's catalog entry, which holds the addresses of its two pictures. */
export function mapCatalogEntryQueryOptions(mapId: string, revision: number) {
  return queryOptions({
    queryKey: mapKeys.entry(mapId, revision),
    queryFn: () => getMapCatalogEntryFn({ data: { mapId, revision } }),
    staleTime: Infinity,
  });
}

export function mapCatalogQueryOptions(search?: string | null, filters?: MapCatalogFilter | null) {
  const normalized = normalizeMapSearch(search);
  const narrowed = normalizeMapCatalogFilters(filters);

  return infiniteQueryOptions({
    queryKey: mapKeys.catalog(normalized, narrowed),
    queryFn: ({ pageParam }) =>
      listMapsFn({
        data: {
          ...(pageParam ? { cursor: pageParam } : {}),
          ...(normalized ? { search: normalized } : {}),
          ...(narrowed.playerCounts.length > 0 ? { playerCounts: narrowed.playerCounts } : {}),
          ...(narrowed.ranks.length > 0 ? { ranks: narrowed.ranks } : {}),
          ...(narrowed.tags.length > 0 ? { tags: narrowed.tags } : {}),
        },
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });
}

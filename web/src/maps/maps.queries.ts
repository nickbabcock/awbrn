import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { normalizeMapSearch } from "./map_catalog.ts";
import { getMapRevisionFn, listMapsFn } from "./maps.functions.ts";
import { mapKeys } from "./maps.keys.ts";

export function mapRevisionQueryOptions(mapId: string, revision: number) {
  return queryOptions({
    queryKey: mapKeys.revision(mapId, revision),
    queryFn: () => getMapRevisionFn({ data: { mapId, revision } }),
    staleTime: Infinity,
  });
}

export function mapCatalogQueryOptions(search?: string | null) {
  const normalized = normalizeMapSearch(search);

  return infiniteQueryOptions({
    queryKey: mapKeys.catalog(normalized),
    queryFn: ({ pageParam }) =>
      listMapsFn({
        data: {
          ...(pageParam ? { cursor: pageParam } : {}),
          ...(normalized ? { search: normalized } : {}),
        },
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });
}

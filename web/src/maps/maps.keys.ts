import { normalizeMapSearch } from "./map_catalog.ts";
import { normalizeMapCatalogFilters } from "./map_taxonomy.ts";
import type { MapCatalogFilter } from "./schemas.ts";

export const mapKeys = {
  all: ["maps"] as const,
  catalog: (search?: string | null, filters?: MapCatalogFilter | null) =>
    [
      ...mapKeys.all,
      "catalog",
      normalizeMapSearch(search),
      normalizeMapCatalogFilters(filters),
    ] as const,
  entry: (mapId: string, revision: number) => [...mapKeys.all, mapId, revision, "entry"] as const,
  revision: (mapId: string, revision: number) => [...mapKeys.all, mapId, revision] as const,
};

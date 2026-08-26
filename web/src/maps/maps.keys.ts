import { normalizeMapSearch } from "./map_catalog.ts";

export const mapKeys = {
  all: ["maps"] as const,
  catalog: (search?: string | null) =>
    [...mapKeys.all, "catalog", normalizeMapSearch(search)] as const,
  revision: (mapId: string, revision: number) => [...mapKeys.all, mapId, revision] as const,
};

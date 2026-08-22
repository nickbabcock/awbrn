import { queryOptions } from "@tanstack/react-query";
import { getMapRevisionFn } from "./maps.functions.ts";

export function mapRevisionQueryOptions(mapId: string, revision: number) {
  return queryOptions({
    queryKey: ["maps", mapId, revision],
    queryFn: () => getMapRevisionFn({ data: { mapId, revision } }),
    staleTime: Infinity,
  });
}

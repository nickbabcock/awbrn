import { createFileRoute } from "@tanstack/react-router";
import {
  mapCatalogQueryOptions,
  mapQueryOptions,
  mapRevisionQueryOptions,
} from "#/maps/maps.queries.ts";
import { mapIdSchema } from "#/maps/schemas.ts";
import { NewMatchPage } from "#/matches/screens/NewMatchPage.tsx";

/**
 * The map the screen opens on, when it was chosen somewhere else.
 *
 * A map's own page hands off to here, so the address carries the map and the
 * screen starts on it. Anything that is not a map id is dropped rather than
 * refused: a bad address opens the board, which is where the screen starts
 * anyway.
 */
export interface NewMatchSearch {
  map?: string;
}

export const Route = createFileRoute("/matches/new")({
  validateSearch: (search: Record<string, unknown>): NewMatchSearch => {
    const chosen = mapIdSchema.safeParse(search.map);
    return chosen.success ? { map: chosen.data } : {};
  },
  loaderDeps: ({ search }) => ({ map: search.map }),
  // The form opens on a map, its seats and its dock in the first paint: the
  // board's first page (whose first card is the default map), the map handed
  // over from its own page, and the chosen map's seats. A read that fails is
  // left to the screen, which says so with a Retry.
  loader: async ({ context, deps }) => {
    const [catalog, handedOver] = await Promise.all([
      context.queryClient.ensureInfiniteQueryData(mapCatalogQueryOptions()).catch(() => null),
      deps.map
        ? context.queryClient.ensureQueryData(mapQueryOptions(deps.map)).catch(() => null)
        : null,
    ]);
    const chosen = handedOver ?? catalog?.pages[0]?.maps[0] ?? null;
    if (chosen) {
      await context.queryClient
        .ensureQueryData(mapRevisionQueryOptions(chosen.mapId, chosen.revision))
        .catch(() => undefined);
    }
  },
  component: NewMatchRouteComponent,
});

function NewMatchRouteComponent() {
  return <NewMatchPage chosenMapId={Route.useSearch().map} />;
}

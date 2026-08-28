import { createFileRoute } from "@tanstack/react-router";
import {
  mapBoardFilters,
  mapBoardSearchText,
  validateMapBoardSearch,
} from "#/maps/map_board_search.ts";
import { mapCatalogQueryOptions } from "#/maps/maps.queries.ts";
import { MapsBoardPage } from "#/maps/screens/MapsBoardPage.tsx";

export const Route = createFileRoute("/maps/")({
  validateSearch: validateMapBoardSearch,
  loaderDeps: ({ search }) => search,
  loader: async ({ context, deps }) => {
    await context.queryClient.ensureInfiniteQueryData(
      mapCatalogQueryOptions(mapBoardSearchText(deps), mapBoardFilters(deps)),
    );
  },
  component: MapsBoardRouteComponent,
});

function MapsBoardRouteComponent() {
  return <MapsBoardPage search={Route.useSearch()} />;
}

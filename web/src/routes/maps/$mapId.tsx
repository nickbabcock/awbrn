import { createFileRoute } from "@tanstack/react-router";
import { mapQueryOptions } from "#/maps/maps.queries.ts";
import { MapPage } from "#/maps/screens/MapPage.tsx";

export const Route = createFileRoute("/maps/$mapId")({
  loader: async ({ context, params }) => {
    await context.queryClient.ensureQueryData(mapQueryOptions(params.mapId));
  },
  component: MapRouteComponent,
});

function MapRouteComponent() {
  return <MapPage mapId={Route.useParams().mapId} />;
}

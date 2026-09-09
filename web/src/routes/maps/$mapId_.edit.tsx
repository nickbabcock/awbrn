import { createFileRoute } from "@tanstack/react-router";
import { mapQueryOptions } from "#/maps/maps.queries.ts";
import { MapEditorPage } from "#/maps/screens/MapEditorPage.tsx";

export const Route = createFileRoute("/maps/$mapId_/edit")({
  loader: async ({ context, params }) => {
    await context.queryClient.ensureQueryData(mapQueryOptions(params.mapId));
  },
  component: EditMapRouteComponent,
});

function EditMapRouteComponent() {
  return <MapEditorPage mapId={Route.useParams().mapId} />;
}

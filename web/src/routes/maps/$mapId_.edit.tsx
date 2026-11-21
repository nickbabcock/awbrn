import { createFileRoute } from "@tanstack/react-router";
import { requireSignIn } from "#/auth/sign_in_redirect.ts";
import { mapQueryOptions } from "#/maps/maps.queries.ts";
import { MapEditorPage } from "#/maps/screens/MapEditorPage.tsx";

export const Route = createFileRoute("/maps/$mapId_/edit")({
  beforeLoad: ({ context, location }) => requireSignIn(context.queryClient, location.href),
  loader: async ({ context, params }) => {
    await context.queryClient.ensureQueryData(mapQueryOptions(params.mapId));
  },
  component: EditMapRouteComponent,
});

function EditMapRouteComponent() {
  return <MapEditorPage mapId={Route.useParams().mapId} />;
}

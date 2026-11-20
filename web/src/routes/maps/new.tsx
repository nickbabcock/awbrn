import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";
import { requireSignIn } from "#/auth/sign_in_redirect.ts";
import { mapIdSchema } from "#/maps/schemas.ts";
import { MapEditorPage } from "#/maps/screens/MapEditorPage.tsx";

/** `from` forks that map: the board opens as it stands and saves as a new map. */
const searchSchema = z.object({ from: mapIdSchema.optional() });

export const Route = createFileRoute("/maps/new")({
  validateSearch: searchSchema,
  beforeLoad: ({ context, location }) => requireSignIn(context.queryClient, location.href),
  component: NewMapRouteComponent,
});

function NewMapRouteComponent() {
  return <MapEditorPage startFrom={Route.useSearch().from} />;
}

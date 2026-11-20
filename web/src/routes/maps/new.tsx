import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";
import { mapIdSchema } from "#/maps/schemas.ts";
import { MapEditorPage } from "#/maps/screens/MapEditorPage.tsx";

/**
 * A board of one's own.
 *
 * `from` is what forking is: the new map opens on somebody else's board and is
 * kept as a map of the player's own, leaving the one they started from alone.
 */
const searchSchema = z.object({ from: mapIdSchema.optional() });

export const Route = createFileRoute("/maps/new")({
  validateSearch: searchSchema,
  component: NewMapRouteComponent,
});

function NewMapRouteComponent() {
  return <MapEditorPage startFrom={Route.useSearch().from} />;
}

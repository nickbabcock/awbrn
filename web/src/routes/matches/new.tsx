import { createFileRoute } from "@tanstack/react-router";
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
  component: NewMatchRouteComponent,
});

function NewMatchRouteComponent() {
  return <NewMatchPage chosenMapId={Route.useSearch().map} />;
}

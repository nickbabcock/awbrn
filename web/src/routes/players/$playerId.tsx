import { createFileRoute, notFound } from "@tanstack/react-router";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { ROW_PREFETCH } from "#/play/prefetch_rows.ts";
import { playerHistoryQueryOptions, playerProfileQueryOptions } from "#/players/players.queries.ts";
import { PlayerProfilePage } from "#/players/screens/PlayerProfilePage.tsx";

export const Route = createFileRoute("/players/$playerId")({
  loader: async ({ context, params }) => {
    const [profile] = await Promise.all([
      context.queryClient.ensureQueryData(playerProfileQueryOptions(params.playerId)),
      context.queryClient.ensureInfiniteQueryData(playerHistoryQueryOptions(params.playerId)),
    ]);
    if (!profile) throw notFound();

    // The leading games in progress top the rail, so their minimaps arrive with it.
    // A failure leaves the row to fetch its own.
    await Promise.all(
      profile.ongoing
        .slice(0, ROW_PREFETCH)
        .map((match) =>
          context.queryClient
            .ensureQueryData(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision))
            .catch(() => undefined),
        ),
    );
  },
  component: PlayerProfileRoute,
});

function PlayerProfileRoute() {
  return <PlayerProfilePage userId={Route.useParams().playerId} />;
}

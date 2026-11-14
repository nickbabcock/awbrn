import { createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { myCompletedMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { MatchHistoryPage } from "#/matches/screens/MatchHistoryPage.tsx";
import { ROW_PREFETCH } from "#/play/prefetch_rows.ts";

export const Route = createFileRoute("/my/history")({
  loader: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    if (!session) {
      throw redirect({ to: "/auth" });
    }

    const history = await context.queryClient.ensureInfiniteQueryData(
      myCompletedMatchesQueryOptions(),
    );
    // Ranked rows are titled by their map, so the leading rows arrive with
    // theirs rather than renaming themselves. A failure leaves the row to fetch.
    await Promise.all(
      (history.pages[0]?.matches ?? [])
        .slice(0, ROW_PREFETCH)
        .map((entry) =>
          context.queryClient
            .ensureQueryData(mapCatalogEntryQueryOptions(entry.mapId, entry.mapRevision))
            .catch(() => undefined),
        ),
    );
  },
  component: MatchHistoryRoute,
});

function MatchHistoryRoute() {
  return <MatchHistoryPage />;
}

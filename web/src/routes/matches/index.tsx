import { createFileRoute } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { matchesBrowseQueryOptions } from "#/matches/matches.queries.ts";
import { MatchesBrowsePage } from "#/matches/screens/MatchesBrowsePage.tsx";
import { prefetchLobbyRows } from "#/play/prefetch_rows.ts";

export const Route = createFileRoute("/matches/")({
  loader: async ({ context }) => {
    const [browse, session] = await Promise.all([
      context.queryClient.ensureInfiniteQueryData(matchesBrowseQueryOptions()),
      context.queryClient.ensureQueryData(sessionQueryOptions()),
    ]);
    // The rows the list opens on arrive whole: map, size, minimap and seats.
    await prefetchLobbyRows(
      context.queryClient,
      browse.pages[0]?.matches ?? [],
      session?.user.id ?? null,
    );
  },
  component: MatchesBrowseRouteComponent,
});

function MatchesBrowseRouteComponent() {
  return <MatchesBrowsePage />;
}

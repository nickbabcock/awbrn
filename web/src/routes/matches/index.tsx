import { createFileRoute } from "@tanstack/react-router";
import { matchesBrowseQueryOptions } from "#/matches/matches.queries.ts";
import { MatchesBrowsePage } from "#/matches/screens/MatchesBrowsePage.tsx";

export const Route = createFileRoute("/matches/")({
  loader: async ({ context }) => {
    await context.queryClient.ensureInfiniteQueryData(matchesBrowseQueryOptions());
  },
  component: MatchesBrowseRouteComponent,
});

function MatchesBrowseRouteComponent() {
  return <MatchesBrowsePage />;
}

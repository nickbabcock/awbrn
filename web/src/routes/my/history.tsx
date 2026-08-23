import { createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { myCompletedMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { MatchHistoryPage } from "#/matches/screens/MatchHistoryPage.tsx";

export const Route = createFileRoute("/my/history")({
  loader: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    if (!session) {
      throw redirect({ to: "/auth" });
    }

    await context.queryClient.ensureInfiniteQueryData(myCompletedMatchesQueryOptions());
  },
  component: MatchHistoryRoute,
});

function MatchHistoryRoute() {
  return <MatchHistoryPage />;
}

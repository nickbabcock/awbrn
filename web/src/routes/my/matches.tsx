import { createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { MyMatchesPage } from "#/matches/screens/MyMatchesPage.tsx";
import { rankedOverviewQueryOptions } from "#/matchmaking/matchmaking.queries.ts";
import { leadingMatches, prefetchMatchRows } from "#/play/prefetch_rows.ts";

export const Route = createFileRoute("/my/matches")({
  loader: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    if (!session) {
      throw redirect({ to: "/auth" });
    }

    const [mine] = await Promise.all([
      context.queryClient.ensureQueryData(myMatchesQueryOptions()),
      // Ranked matches are titled by their map, which the overview says.
      context.queryClient.ensureQueryData(rankedOverviewQueryOptions()).catch(() => undefined),
    ]);
    // The rows the screen opens on arrive whole: each map's minimap and each
    // table's opponents, not paper wells filling in.
    await prefetchMatchRows(context.queryClient, leadingMatches(mine.matches));
  },
  component: MyMatchesRoute,
});

function MyMatchesRoute() {
  return <MyMatchesPage />;
}

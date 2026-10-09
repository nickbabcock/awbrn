import { createFileRoute } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { HomePage } from "#/home/HomePage.tsx";
import { matchesBrowseQueryOptions, myMatchesQueryOptions } from "#/matches/matches.queries.ts";

export const Route = createFileRoute("/")({
  // The queue is the first thing the screen draws, so it arrives with the
  // page rather than pushing the briefing down a moment later.
  loader: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    await Promise.all([
      context.queryClient.ensureInfiniteQueryData(matchesBrowseQueryOptions()),
      session ? context.queryClient.ensureQueryData(myMatchesQueryOptions()) : null,
    ]);
  },
  component: HomePage,
});

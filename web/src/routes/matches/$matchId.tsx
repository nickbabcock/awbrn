import { createFileRoute } from "@tanstack/react-router";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { MatchLobbyPage } from "#/matches/screens/MatchLobbyPage.tsx";

type MatchSearch = {
  join?: string;
};

export const Route = createFileRoute("/matches/$matchId")({
  validateSearch: (search: Record<string, unknown>): MatchSearch => ({
    join: typeof search.join === "string" && search.join.length > 0 ? search.join : undefined,
  }),
  loaderDeps: ({ search }) => ({
    joinSlug: search.join ?? null,
  }),
  loader: async ({ context, deps, params }) => {
    await context.queryClient.ensureQueryData(
      matchDetailQueryOptions(params.matchId, deps.joinSlug),
    );
  },
  component: MatchLobbyRouteComponent,
});

function MatchLobbyRouteComponent() {
  const { matchId } = Route.useParams();
  const search = Route.useSearch();
  return <MatchLobbyPage matchId={matchId} joinSlug={search.join ?? null} />;
}

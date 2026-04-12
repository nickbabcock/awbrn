import { createFileRoute } from "@tanstack/react-router";
import { useSuspenseQuery } from "@tanstack/react-query";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { MatchActivePage } from "#/matches/screens/MatchActivePage.tsx";
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
  component: MatchRouteComponent,
});

function MatchRouteComponent() {
  const { matchId } = Route.useParams();
  const search = Route.useSearch();
  const joinSlug = search.join ?? null;
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));

  if (match.phase === "active") {
    return <MatchActivePage matchId={matchId} />;
  }
  return <MatchLobbyPage matchId={matchId} joinSlug={joinSlug} />;
}

import { createFileRoute } from "@tanstack/react-router";
import { MatchLobbyPage } from "../../matches/screens/MatchLobbyPage";

type MatchSearch = {
  join?: string;
};

export const Route = createFileRoute("/matches/$matchId")({
  validateSearch: (search: Record<string, unknown>): MatchSearch => ({
    join: typeof search.join === "string" && search.join.length > 0 ? search.join : undefined,
  }),
  component: MatchLobbyRouteComponent,
});

function MatchLobbyRouteComponent() {
  const { matchId } = Route.useParams();
  const search = Route.useSearch();
  return <MatchLobbyPage matchId={matchId} joinSlug={search.join ?? null} />;
}

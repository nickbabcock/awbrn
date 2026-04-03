import { createFileRoute } from "@tanstack/react-router";
import { MatchLobbyPage } from "../../pages/MatchLobbyPage";

export const Route = createFileRoute("/matches/$matchId")({
  component: MatchLobbyRouteComponent,
});

function MatchLobbyRouteComponent() {
  const { matchId } = Route.useParams();
  return <MatchLobbyPage matchId={matchId} />;
}

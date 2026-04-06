import { createFileRoute } from "@tanstack/react-router";
import { MatchesBrowsePage } from "#/matches/screens/MatchesBrowsePage.tsx";

export const Route = createFileRoute("/matches/")({
  component: MatchesBrowsePage,
});

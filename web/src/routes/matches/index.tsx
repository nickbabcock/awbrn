import { createFileRoute } from "@tanstack/react-router";
import { listMatchesFn } from "#/matches/matches.functions.ts";
import { MatchesBrowsePage } from "#/matches/screens/MatchesBrowsePage.tsx";

export const Route = createFileRoute("/matches/")({
  loader: () => listMatchesFn({ data: {} }),
  component: MatchesBrowseRouteComponent,
});

function MatchesBrowseRouteComponent() {
  const initialData = Route.useLoaderData();
  return <MatchesBrowsePage initialData={initialData} />;
}

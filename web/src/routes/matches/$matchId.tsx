import { createFileRoute } from "@tanstack/react-router";
import { useSuspenseQuery } from "@tanstack/react-query";
import { mapCatalogEntryQueryOptions, mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { MatchActivePage } from "#/matches/screens/MatchActivePage.tsx";
import { MatchLobbyPage } from "#/matches/screens/MatchLobbyPage.tsx";
import { RankedPendingPage } from "#/matchmaking/screens/RankedPendingPage.tsx";

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
    const match = await context.queryClient.ensureQueryData(
      matchDetailQueryOptions(params.matchId, deps.joinSlug),
    );

    // A briefing is about its map, so the map arrives with the first paint
    // rather than replacing an identifier a moment later. A map that cannot be
    // read is not a reason to fail the page: the screen has its own fallback.
    if (match.phase === "lobby" || match.phase === "pending") {
      await Promise.all([
        context.queryClient
          .ensureQueryData(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision))
          .catch(() => undefined),
        context.queryClient
          .ensureQueryData(mapRevisionQueryOptions(match.mapId, match.mapRevision))
          .catch(() => undefined),
      ]);
    }
  },
  component: MatchRouteComponent,
});

function MatchRouteComponent() {
  const { matchId } = Route.useParams();
  const search = Route.useSearch();
  const joinSlug = search.join ?? null;
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));

  if (match.phase === "pending") {
    // A ranked pairing is the only match that reaches this phase. It has no
    // host, no seats to claim, and no invite, so it does not use the lobby.
    return <RankedPendingPage joinSlug={joinSlug} matchId={matchId} />;
  }

  if (match.phase === "active") {
    // The slug travels with the viewer: a private match stays unreadable to a
    // non-participant without it, and the invite link is the only way in.
    return <MatchActivePage joinSlug={joinSlug} matchId={matchId} />;
  }
  return <MatchLobbyPage matchId={matchId} joinSlug={joinSlug} />;
}

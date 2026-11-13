import { createFileRoute } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { chooseFirstMatchMap, chooseLandingMaps } from "#/home/depicted.ts";
import { HomePage } from "#/home/HomePage.tsx";
import { rankedMapsQueryOptions, ringDateText } from "#/home/Landing.tsx";
import { mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import { matchesBrowseQueryOptions, myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { rankedOverviewQueryOptions } from "#/matchmaking/matchmaking.queries.ts";
import { leadingMatches, prefetchLobbyRows, prefetchMatchRows } from "#/play/prefetch_rows.ts";

export const Route = createFileRoute("/")({
  // The queue is the first thing the screen draws, so it arrives with the
  // page rather than pushing the briefing down a moment later.
  loader: async ({ context }) => {
    const { queryClient } = context;
    const session = await queryClient.ensureQueryData(sessionQueryOptions());
    const [browse, mine, maps] = await Promise.all([
      queryClient.ensureInfiniteQueryData(matchesBrowseQueryOptions()),
      session ? queryClient.ensureQueryData(myMatchesQueryOptions()) : null,
      queryClient.ensureInfiniteQueryData(rankedMapsQueryOptions()),
      // Ranked matches are titled by their map and a pairing's window comes
      // from the ranked overview, so it arrives with the queue it labels.
      session
        ? queryClient.ensureQueryData(rankedOverviewQueryOptions()).catch(() => undefined)
        : null,
    ]);
    // The maps the landing depicts and the one a first match is played on:
    // their terrain places the depicted units and their seats give the armies.
    const catalog = maps.pages.flatMap((page) => page.maps);
    const landing = mine === null ? chooseLandingMaps(catalog) : null;
    const depicted = landing
      ? [landing.featured, landing.replay]
      : mine?.matches.length === 0
        ? [chooseFirstMatchMap(catalog)]
        : [];
    // What waits on the player leads the screen, so its rows arrive whole.
    // Open lobbies follow it (the rail, or the first match's rows).
    const lobbies = (browse.pages[0]?.matches ?? []).filter(
      (lobby) => !mine?.matches.some((match) => match.matchId === lobby.matchId),
    );
    await Promise.all([
      ...[...new Set(depicted)].map((map) =>
        map
          ? queryClient
              .ensureQueryData(mapRevisionQueryOptions(map.mapId, map.revision))
              .catch(() => undefined)
          : null,
      ),
      mine !== null
        ? prefetchMatchRows(queryClient, leadingMatches(mine.matches, { isOwedOnly: true }))
        : null,
      prefetchLobbyRows(queryClient, lobbies, session?.user.id ?? null),
    ]);
    return { ringDate: ringDateText(new Date()) };
  },
  component: HomePage,
});

import type { QueryClient } from "@tanstack/react-query";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { needsViewerAction } from "#/matches/my_matches.ts";
import type { MatchBrowseSummary, MyMatchSummary } from "#/matches/schemas.ts";

/** How many match rows a screen loads whole before it draws. */
export const ROW_PREFETCH = 8;

/**
 * The rows that lead a list of the viewer's matches: what waits on them
 * first, then the rest, the order the list draws them in.
 */
export function leadingMatches(
  matches: readonly MyMatchSummary[],
  { isOwedOnly = false }: { isOwedOnly?: boolean } = {},
): MyMatchSummary[] {
  const owed = matches.filter(needsViewerAction);
  const lead = isOwedOnly
    ? owed
    : [...owed, ...matches.filter((match) => !needsViewerAction(match))];
  return lead.slice(0, ROW_PREFETCH);
}

/**
 * Loads what the leading match rows draw, so they arrive whole: each map's
 * minimap and each table's opponents, not empty wells filling in. A lobby
 * still filling shows its seats, which the list already carries. A failure
 * leaves the row to fetch on its own.
 */
export async function prefetchMatchRows(
  queryClient: QueryClient,
  matches: readonly MyMatchSummary[],
): Promise<void> {
  await Promise.all(
    matches.flatMap((match) => [
      queryClient
        .ensureQueryData(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision))
        .catch(() => undefined),
      match.phase === "lobby" || match.phase === "draft"
        ? null
        : queryClient
            .ensureQueryData(matchDetailQueryOptions(match.matchId, null))
            .catch(() => undefined),
    ]),
  );
}

/**
 * Loads what the leading open-lobby rows draw: each map's minimap, name and
 * size, so the rows arrive whole. The list already carries the seats; only a
 * lobby the viewer sits in reads its match record, for the viewer's own
 * commander, which the lobby seals from everyone else. A failure leaves the
 * row to fetch on its own.
 */
export async function prefetchLobbyRows(
  queryClient: QueryClient,
  lobbies: readonly MatchBrowseSummary[],
  viewerId: string | null,
): Promise<void> {
  await Promise.all(
    lobbies
      .slice(0, ROW_PREFETCH)
      .flatMap((lobby) => [
        queryClient
          .ensureQueryData(mapCatalogEntryQueryOptions(lobby.mapId, lobby.mapRevision))
          .catch(() => undefined),
        isSeatedIn(lobby, viewerId)
          ? queryClient
              .ensureQueryData(matchDetailQueryOptions(lobby.matchId, null))
              .catch(() => undefined)
          : null,
      ]),
  );
}

/** Whether the viewer holds a seat in this open lobby. */
export function isSeatedIn(lobby: MatchBrowseSummary, viewerId: string | null): boolean {
  return viewerId !== null && lobby.seats.some((seat) => seat.userId === viewerId);
}

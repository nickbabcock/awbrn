import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import {
  getMatchFn,
  listMatchesFn,
  listMyCompletedMatchesFn,
  listMyMatchesFn,
  matchesAwaitingViewerFn,
} from "./matches.functions";
import { matchKeys, normalizeJoinSlug } from "./matches.keys";
import type { MyMatchesResponse } from "./schemas";

export interface MyMatchesQueryResponse extends MyMatchesResponse {
  loadedAt: string;
}

export function matchesBrowseQueryOptions() {
  return infiniteQueryOptions({
    queryKey: matchKeys.browse(),
    queryFn: ({ pageParam }) => {
      return listMatchesFn({ data: pageParam ? { cursor: pageParam } : {} });
    },
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });
}

export function myMatchesQueryOptions() {
  return queryOptions({
    queryKey: matchKeys.mine(),
    queryFn: async (): Promise<MyMatchesQueryResponse> => {
      const data = await listMyMatchesFn();
      return {
        ...data,
        loadedAt: new Date().toISOString(),
      };
    },
  });
}

/**
 * How much is waiting on the viewer, as the nav badge and the tab report it.
 *
 * The player's own socket says when this has changed, so there is no interval
 * to guess at: the count is re-read when a match reports that it moved, and
 * the write it reports has already landed by the time the tab hears about it.
 *
 * `isLive` is whether that socket is up. While it is not, the count falls back
 * to asking, because a badge that is only correct when a connection holds is
 * worse than one that is a minute behind. Returning to the tab re-reads either
 * way, and it is opted into here because the router turns it off everywhere
 * else.
 */
export function matchesAwaitingQueryOptions(isLive: boolean) {
  return queryOptions({
    queryKey: matchKeys.awaiting(),
    queryFn: () => matchesAwaitingViewerFn(),
    refetchOnWindowFocus: true,
    refetchInterval: isLive ? false : AWAITING_FALLBACK_POLL_MS,
  });
}

/** How often the count is asked for while the player has no socket. */
const AWAITING_FALLBACK_POLL_MS = 60_000;

export function myCompletedMatchesQueryOptions() {
  return infiniteQueryOptions({
    queryKey: matchKeys.completed(),
    queryFn: ({ pageParam }) =>
      listMyCompletedMatchesFn({ data: pageParam ? { cursor: pageParam } : {} }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });
}

export function matchDetailQueryOptions(matchId: string, joinSlug: string | null | undefined) {
  const normalizedJoinSlug = normalizeJoinSlug(joinSlug);

  return queryOptions({
    queryKey: matchKeys.detail(matchId, normalizedJoinSlug),
    queryFn: () => getMatchFn({ data: { matchId, joinSlug: normalizedJoinSlug } }),
  });
}

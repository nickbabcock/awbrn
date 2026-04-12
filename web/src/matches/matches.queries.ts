import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { getMatchFn, listMatchesFn, listMyMatchesFn } from "./matches.functions";
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

export function matchDetailQueryOptions(matchId: string, joinSlug: string | null | undefined) {
  const normalizedJoinSlug = normalizeJoinSlug(joinSlug);

  return queryOptions({
    queryKey: matchKeys.detail(matchId, normalizedJoinSlug),
    queryFn: () => getMatchFn({ data: { matchId, joinSlug: normalizedJoinSlug } }),
  });
}

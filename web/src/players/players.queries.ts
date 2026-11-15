import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { getPlayerProfileFn, listPlayerCompletedMatchesFn } from "./players.functions.ts";
import { playerKeys } from "./players.keys.ts";

export function playerProfileQueryOptions(userId: string) {
  return queryOptions({
    queryKey: playerKeys.profile(userId),
    queryFn: () => getPlayerProfileFn({ data: { userId } }),
  });
}

export function playerHistoryQueryOptions(userId: string) {
  return infiniteQueryOptions({
    queryKey: playerKeys.history(userId),
    queryFn: ({ pageParam }) =>
      listPlayerCompletedMatchesFn({
        data: pageParam ? { userId, cursor: pageParam } : { userId },
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });
}

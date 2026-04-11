import { queryOptions } from "@tanstack/react-query";
import { getSessionFn } from "./auth.functions";
import { authKeys } from "./auth.keys";

export function sessionQueryOptions() {
  return queryOptions({
    queryKey: authKeys.session(),
    queryFn: () => getSessionFn(),
  });
}

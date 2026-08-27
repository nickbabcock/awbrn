import { queryOptions } from "@tanstack/react-query";
import { listModerationActionsFn } from "./moderation.functions.ts";
import { moderationKeys } from "./moderation.keys.ts";
import type { ModerationLogRequest } from "./schemas.ts";

export function moderationLogQueryOptions(request: ModerationLogRequest = {}) {
  return queryOptions({
    queryKey: moderationKeys.log(request),
    queryFn: () => listModerationActionsFn({ data: request }),
  });
}

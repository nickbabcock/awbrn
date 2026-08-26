import type { ModerationLogRequest } from "./schemas.ts";

export const moderationKeys = {
  all: ["moderation"] as const,
  log: (request: ModerationLogRequest = {}) => [...moderationKeys.all, "log", request] as const,
};

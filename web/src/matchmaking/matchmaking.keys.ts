import type { RankedPool } from "#/matches/schemas.ts";

export const rankedKeys = {
  all: ["ranked"] as const,
  overview: () => [...rankedKeys.all, "overview"] as const,
  standings: (pool: RankedPool) => [...rankedKeys.all, "standings", pool] as const,
};

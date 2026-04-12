export function normalizeJoinSlug(joinSlug: string | null | undefined): string | null {
  return joinSlug && joinSlug.length > 0 ? joinSlug : null;
}

export const matchKeys = {
  all: ["matches"] as const,
  browse: () => [...matchKeys.all, "browse"] as const,
  mine: () => [...matchKeys.all, "mine"] as const,
  details: () => [...matchKeys.all, "detail"] as const,
  detail: (matchId: string, joinSlug: string | null | undefined) =>
    [...matchKeys.details(), matchId, normalizeJoinSlug(joinSlug)] as const,
};

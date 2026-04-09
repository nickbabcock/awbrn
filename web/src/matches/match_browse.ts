export const MATCH_BROWSE_PAGE_SIZE = 12;

export interface MatchBrowseCursor {
  createdAt: string;
  matchId: string;
}

export function encodeMatchBrowseCursor(cursor: MatchBrowseCursor): string {
  return JSON.stringify(cursor);
}

export function decodeMatchBrowseCursor(cursor: string | undefined): MatchBrowseCursor | null {
  if (!cursor) {
    return null;
  }

  try {
    const value = JSON.parse(cursor) as Partial<MatchBrowseCursor>;
    if (
      typeof value.createdAt === "string" &&
      value.createdAt.length > 0 &&
      typeof value.matchId === "string" &&
      value.matchId.length > 0
    ) {
      return {
        createdAt: value.createdAt,
        matchId: value.matchId,
      };
    }
  } catch {}

  return null;
}

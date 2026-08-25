import { z } from "zod";
import { decodeCursor } from "./cursor";
import { matchIdSchema } from "./match_id";

export const MATCH_BROWSE_PAGE_SIZE = 12;

const matchBrowseCursorSchema = z.object({
  createdAt: z.iso.datetime(),
  matchId: matchIdSchema,
});

export type MatchBrowseCursor = z.infer<typeof matchBrowseCursorSchema>;

export function encodeMatchBrowseCursor(cursor: MatchBrowseCursor): string {
  return JSON.stringify(cursor);
}

export function decodeMatchBrowseCursor(cursor: string | undefined): MatchBrowseCursor | null {
  return decodeCursor(cursor, matchBrowseCursorSchema);
}

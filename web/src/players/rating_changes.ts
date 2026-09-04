import { create } from "zustand";
import type { RatingChangedNotification } from "./player_protocol.ts";

interface RatingChangeState {
  /** The rating move a match made, once its pool has applied it. */
  byMatchId: Record<string, RatingChangedNotification>;
  record: (change: RatingChangedNotification) => void;
}

/**
 * Rating moves this tab has been told about, kept by the match that caused them.
 *
 * A rating is applied after the match ends, by the durable object that owns the
 * pool, so the report of a match is usually open before the number moves. The
 * player's own socket announces the move when it lands; this is where the
 * announcement waits for whichever page wants to show it.
 *
 * It is deliberately not a query: nothing here is fetched, and a page that
 * missed the announcement is not one that should ask again for it. The rating
 * is written down, and reopening the match reads it from the record.
 */
export const useRatingChanges = create<RatingChangeState>((set) => ({
  byMatchId: {},
  record: (change) =>
    set((state) => ({ byMatchId: { ...state.byMatchId, [change.matchId]: change } })),
}));

/** The rating move this match made for the viewer, or null if none has landed. */
export function useRatingChange(matchId: string): RatingChangedNotification | null {
  return useRatingChanges((state) => state.byMatchId[matchId] ?? null);
}

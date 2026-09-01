/**
 * Who holds a seat, read off a row and written back onto one.
 *
 * A seat is held by an occupant, and a person is one kind of occupant. The
 * database says which by which column it filled: a row carries a `userId` or
 * an `aiProfileId` and never both, which a check constraint enforces. That is
 * the whole model. There is no third column repeating which of the two it is,
 * because a discriminator that has to agree with the data is one more thing
 * that can disagree with it.
 *
 * What this buys is that every query which means "a person" already says so.
 * A rating, a ban, a moderation record and a leaderboard all name `userId`,
 * and an opponent has none, so none of them has to remember to exclude one.
 */

import { aiProfileIds, type AiProfileId, type SeatOccupant } from "./schemas.ts";

/** The two columns a seat's occupant is stored in. */
export interface SeatOccupantColumns {
  userId: string | null;
  aiProfileId: string | null;
}

/** Whether this identifier names an opponent this build knows. */
export function isAiProfileId(id: string | null): id is AiProfileId {
  return id !== null && (aiProfileIds as readonly string[]).includes(id);
}

/**
 * The occupant a row describes, or null for a row that fills neither column.
 *
 * A row that fills neither cannot exist while the check constraint holds, so
 * the null is what a caller reading an unmigrated or hand-edited row gets
 * rather than a throw in the middle of rendering a lobby.
 */
export function seatOccupant(columns: SeatOccupantColumns): SeatOccupant | null {
  if (columns.userId !== null) {
    return { kind: "human", userId: columns.userId };
  }
  if (isAiProfileId(columns.aiProfileId)) {
    return { kind: "ai", profileId: columns.aiProfileId };
  }
  return null;
}

/** The columns that store this occupant. */
export function occupantColumns(occupant: SeatOccupant): SeatOccupantColumns {
  return occupant.kind === "human"
    ? { userId: occupant.userId, aiProfileId: null }
    : { userId: null, aiProfileId: occupant.profileId };
}

/** The person holding this seat, or null when the server plays it. */
export function occupantUserId(occupant: SeatOccupant | null): string | null {
  return occupant?.kind === "human" ? occupant.userId : null;
}

/** Whether the server plays this seat, and so owes it a turn. */
export function isServerPlayed(columns: SeatOccupantColumns): boolean {
  return columns.userId === null && isAiProfileId(columns.aiProfileId);
}

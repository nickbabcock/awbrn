/*
 * What a viewer is told about a match they can read.
 *
 * One rule lives here, and it is the reason the module is separate from the
 * database code that calls it: a pending ranked pairing shows the match and
 * not the person.
 *
 * The confirmation window is the one moment in ranked play a player can walk
 * away from at no cost. If the other player's name were readable in that
 * window, a player could decline the opponents they do not want and accept the
 * rest, which is a real risk while the pool is small enough that everybody
 * knows everybody. So the seat, the creator fields, and the commander are all
 * withheld until both players are ready and the match can no longer be
 * refused. The fields are replaced here rather than hidden in the interface,
 * because a name that ships inside the payload is not hidden.
 */

import type { MatchSnapshot } from "./schemas.ts";

export const HIDDEN_OPPONENT_NAME = "Opponent";

/** A stand-in identity, so that no real user id travels with a pairing. */
export function hiddenOpponentUserId(slotIndex: number): string {
  return `hidden:${slotIndex}`;
}

/**
 * Whether this snapshot still withholds the other player.
 *
 * Only a ranked pairing reaches the pending phase, and it leaves that phase
 * the moment both players are ready.
 */
export function hidesOpponent(snapshot: MatchSnapshot): boolean {
  return (
    snapshot.phase === "pending" && !snapshot.participants.every((participant) => participant.ready)
  );
}

export function applyViewerVisibility(
  snapshot: MatchSnapshot,
  viewerUserId: string | null,
): MatchSnapshot {
  const joinSlug = viewerUserId === snapshot.creatorUserId ? snapshot.joinSlug : null;
  if (!hidesOpponent(snapshot)) return { ...snapshot, joinSlug };

  // The creator of a ranked match is one of its two players, so the creator
  // fields name the opponent as surely as the seat does.
  const hideCreator = snapshot.creatorUserId !== viewerUserId;
  return {
    ...snapshot,
    joinSlug,
    creatorUserId: hideCreator ? hiddenOpponentUserId(0) : snapshot.creatorUserId,
    creatorName: hideCreator ? HIDDEN_OPPONENT_NAME : snapshot.creatorName,
    participants: snapshot.participants.map((participant) =>
      participant.userId === viewerUserId
        ? participant
        : {
            ...participant,
            userId: hiddenOpponentUserId(participant.slotIndex),
            userName: HIDDEN_OPPONENT_NAME,
            coId: null,
          },
    ),
  };
}

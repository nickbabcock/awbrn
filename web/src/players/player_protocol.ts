/**
 * What a player's durable object says to that player's open tabs.
 *
 * The match durable object is the authority on whose turn it is; these are
 * announcements that it has changed, not the state itself. A tab that hears
 * one re-reads the counts it shows, which is why nothing here carries a count.
 */

/** A turn has opened for this player. Worth interrupting them for. */
export interface TurnStartedNotification {
  type: "turnStarted";
  matchId: string;
  matchName: string;
  /** When the turn runs out, in milliseconds, or null with no clock. */
  deadlineAt: number | null;
}

/** A turn of this player's has closed. Worth a quieter refresh. */
export interface TurnEndedNotification {
  type: "turnEnded";
  matchId: string;
}

export type PlayerNotification = TurnStartedNotification | TurnEndedNotification;

/** The opening frame, so a tab knows the socket is live and not merely open. */
export interface PlayerReadyMessage {
  type: "ready";
}

export type PlayerSocketMessage = PlayerNotification | PlayerReadyMessage;

/**
 * A tab reporting whether the player can see it.
 *
 * A hidden tab is an open socket the player is not reading, so it is told what
 * changed and the player is still sent a notification. Without this a player
 * with a forgotten tab would never hear anything.
 */
export interface PlayerVisibilityMessage {
  type: "visibility";
  visible: boolean;
}

export type PlayerClientMessage = PlayerVisibilityMessage;

/**
 * What the service worker receives and draws a notification from.
 *
 * The wording is settled here rather than in the service worker, which has no
 * build step of its own and would otherwise be a second place these sentences
 * are written. It draws what it is given.
 */
export interface PushDigestPayload {
  type: "turnDigest";
  /** The line the notification leads with. */
  title: string;
  /** What it says underneath. */
  body: string;
  /** Where clicking it takes the player. */
  url: string;
  /** How many matches are waiting, which can exceed the ones named. */
  total: number;
}

/** How many matches a single notification names before it only counts them. */
export const PUSH_DIGEST_LIMIT = 3;

export function parsePlayerClientMessage(raw: unknown): PlayerClientMessage | null {
  if (typeof raw !== "object" || raw === null) return null;
  const message = raw as Partial<PlayerVisibilityMessage>;
  if (message.type === "visibility" && typeof message.visible === "boolean") {
    return { type: "visibility", visible: message.visible };
  }
  return null;
}

/** One match a player has been left to play. */
export interface WaitingMatch {
  matchId: string;
  matchName: string;
}

/**
 * Write the notification for the matches waiting on a player.
 *
 * A player with one waiting match is sent to it, and one with several is sent
 * to the list, because a notification that names three matches cannot open
 * three of them.
 */
export function buildTurnDigest(waiting: WaitingMatch[]): PushDigestPayload {
  const named = waiting.slice(0, PUSH_DIGEST_LIMIT);
  const names = named.map((match) => match.matchName);
  const remaining = waiting.length - named.length;

  return {
    type: "turnDigest",
    title: waiting.length === 1 ? "Your turn" : `Your turn in ${waiting.length} matches`,
    body: remaining > 0 ? `${names.join(", ")} and ${remaining} more` : names.join(", "),
    url:
      waiting.length === 1 && waiting[0] !== undefined
        ? `/matches/${waiting[0].matchId}`
        : "/my/matches",
    total: waiting.length,
  };
}

import type { MatchSnapshot } from "./schemas";

/**
 * Starting resolves in one round trip, and reading the record is what drives it
 * (the snapshot read finalizes a match stuck in `starting`), so this phase is
 * polled tightly and briefly.
 */
export const STARTING_POLL_INTERVAL_MS = 2_000;

/**
 * How often to re-read a waiting lobby, by how long it has been since anything
 * in it changed.
 *
 * A lobby is not a countdown. Play-by-web opponents ready up over hours or
 * days, so a fixed interval is either too slow to feel live while people are
 * actively arriving, or a page left open overnight making thousands of pointless
 * requests. Attention decays with silence: seconds after a change, then minutes.
 * Returning to the tab refetches on focus regardless, which is how a wait
 * measured in days actually ends.
 */
const LOBBY_POLL_LADDER: readonly { readonly quietForMs: number; readonly everyMs: number }[] = [
  { quietForMs: 45_000, everyMs: 4_000 },
  { quietForMs: 5 * 60_000, everyMs: 15_000 },
  { quietForMs: 30 * 60_000, everyMs: 60_000 },
];

const LOBBY_POLL_FLOOR_MS = 5 * 60_000;

export function lobbyPollInterval(quietForMs: number): number {
  for (const step of LOBBY_POLL_LADDER) {
    if (quietForMs <= step.quietForMs) {
      return step.everyMs;
    }
  }

  return LOBBY_POLL_FLOOR_MS;
}

/**
 * Everything a waiting player is waiting on. Compared between reads so the poll
 * knows whether the lobby is alive or quiet.
 */
export function lobbySignature(match: MatchSnapshot): string {
  return JSON.stringify([
    match.phase,
    match.participants.map((participant) => [
      participant.slotIndex,
      participant.userId,
      participant.ready,
      participant.coId,
      participant.factionId,
    ]),
  ]);
}

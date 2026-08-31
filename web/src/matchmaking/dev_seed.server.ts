/**
 * Two development accounts looking for a ranked match.
 *
 * The ranked screens are only worth opening when somebody is on them, and a
 * pairing is not a row that can honestly be written by hand: the seats take
 * the map's own factions, the confirmation window runs on a clock, and the
 * match itself is built by the durable object rather than by an insert. So
 * the seed does not build a pairing. It makes two accounts seek, and lets the
 * matchmaker build whatever it would have built for two real players.
 *
 * Nothing here needs to be re-run or re-stamped. A pairing that already
 * exists stops another one being made, because the matchmaking pass refuses a
 * pair that already holds a live match, and a match that has been played out
 * pairs the two of them again the next time the server starts.
 */

import { activeSeasonNumber, startSeek } from "./matchmaking.server.ts";

/** The two seeded accounts that stand in the ranked pool. */
const DEV_SEEKERS = ["player@awbrn.test", "rival@awbrn.test"] as const;

let seeded: Promise<void> | null = null;

/**
 * Put the seeded players in the standard async pool.
 *
 * `accounts` maps a seeded email to the id it holds, which is what
 * `seedDevAccounts` reports.
 *
 * A failed seed is logged and dropped. A development server that cannot fill
 * the ranked pool should still answer for every other screen.
 */
export function seedDevRankedSeeks(accounts: ReadonlyMap<string, string>): Promise<void> {
  // Once for each server that starts, and not once for each request: a seek
  // wakes the matchmaker, which is real work and not work a page view should
  // ask for. A failed seed is not remembered, so the next request tries again.
  seeded ??= runSeed(accounts).catch((error: unknown) => {
    seeded = null;
    console.error("[dev-seed] could not seed the ranked pool", error);
  });
  return seeded;
}

async function runSeed(accounts: ReadonlyMap<string, string>): Promise<void> {
  // Seeking is refused while no season is open, and the season is written by
  // the map seed. A run that finds none has done nothing, so it is not
  // remembered either: the next request looks again.
  if ((await activeSeasonNumber()) === null) {
    seeded = null;
    return;
  }

  // One at a time, because the second seek is what pairs them: `startSeek`
  // wakes the matchmaker, and the pass it runs needs both seeks in hand.
  for (const email of DEV_SEEKERS) {
    const userId = accounts.get(email);
    if (!userId) continue;
    await startSeek(userId, "async");
  }
  console.log(`[dev-seed] ranked pool async holds seeks for ${DEV_SEEKERS.join(", ")}`);
}

import { DurableObject } from "cloudflare:workers";
import type { RankedPool } from "#/matches/schemas.ts";
import { rankedPools } from "#/matches/schemas.ts";
import { getPlayerStubFrom } from "#/players/player_service.ts";
import { applyPendingRatings, captureClosedSeasons, type AppliedRating } from "./ratings.server.ts";

const IDENTITY_KEY = "pool";

/** How many passes one wake makes before it leaves the rest for the next. */
const MAX_PASSES_PER_INVOCATION = 10;

/**
 * How long after a pass fails the object tries again.
 *
 * A match which has ended is already recorded, so a rating which arrives a
 * minute late costs the player nothing but the wait.
 */
const RETRY_DELAY_MS = 60_000;

/**
 * How long after a wake which ran out of passes the object comes back.
 *
 * The work is waiting and nothing has failed, so the wait is short. It is not
 * none: the alarm hands the rest of the queue to a fresh invocation instead of
 * holding one open past the time it is given.
 */
const CONTINUE_DELAY_MS = 1_000;

/**
 * The single writer for one pool's ratings.
 *
 * Matches end in their own durable objects, all at once and with no order
 * between them, so the rating rows they share would be written by many workers
 * at the same time. Every one of those writes is sent here instead. One object
 * for each pool, one operation at a time, means a rating is read and written
 * by one caller and the read-modify-write in `applyPendingRatings` needs no
 * version to compare against.
 *
 * The object holds no state of its own. D1 stays the record: `match_results`
 * is the queue and the receipt, so a wake which never arrives costs nothing
 * more than the wait until the next one.
 */
export class RatingsDurableObject extends DurableObject<CloudflareBindings> {
  private tail: Promise<void> = Promise.resolve();

  /** Rate what is waiting. Safe to call again, and safe to call twice. */
  async kick(pool: RankedPool): Promise<number> {
    if (!rankedPools.includes(pool)) throw new Error("unknown ranked pool");
    return this.serialized(async () => {
      await this.ctx.storage.put(IDENTITY_KEY, pool);
      return this.drain(pool);
    });
  }

  async alarm(): Promise<void> {
    await this.serialized(async () => {
      const pool = await this.ctx.storage.get<RankedPool>(IDENTITY_KEY);
      if (!pool) return;
      try {
        await this.drain(pool);
      } catch (error) {
        await this.ctx.storage.setAlarm(Date.now() + RETRY_DELAY_MS);
        throw error;
      }
    });
  }

  private async drain(pool: RankedPool): Promise<number> {
    const applied: AppliedRating[] = [];
    try {
      let drained = false;
      for (let pass = 0; pass < MAX_PASSES_PER_INVOCATION; pass += 1) {
        const result = await applyPendingRatings(this.env.DB, pool);
        applied.push(...result.applied);
        if (result.drained) {
          drained = true;
          break;
        }
      }
      // A season is frozen only once every result of it has been rated, so
      // this runs after the passes and not before them.
      await captureClosedSeasons(this.env.DB, pool);
      // The alarm goes only when the queue is empty. A wake which used every
      // pass and still found more keeps one, or the rest of the queue would
      // wait for a match of this pool to end.
      if (drained) await this.ctx.storage.deleteAlarm();
      else await this.ctx.storage.setAlarm(Date.now() + CONTINUE_DELAY_MS);
    } catch (error) {
      // The results stay unstamped, so the next wake reads them again.
      await this.ctx.storage.setAlarm(Date.now() + RETRY_DELAY_MS);
      await this.announce(applied);
      throw error;
    }

    await this.announce(applied);
    return applied.length;
  }

  /**
   * Tell each player their rating moved.
   *
   * A tab which is reading the report of the match fills the number in. The
   * rating is already written, so a message which does not arrive loses
   * nothing that reloading the page does not recover.
   */
  private async announce(applied: readonly AppliedRating[]): Promise<void> {
    if (applied.length === 0) return;
    const settled = await Promise.allSettled(
      applied.map((change) =>
        getPlayerStubFrom(this.env.PLAYERS, change.userId).notify({
          type: "ratingChanged",
          matchId: change.matchId,
          pool: change.pool,
          ratingBefore: change.ratingBefore,
          ratingAfter: change.ratingAfter,
        }),
      ),
    );
    for (const result of settled) {
      if (result.status === "rejected") {
        console.error("Failed to announce a rating change to a player:", result.reason);
      }
    }
  }

  private async serialized<T>(operation: () => Promise<T>): Promise<T> {
    const previous = this.tail;
    let release!: () => void;
    this.tail = new Promise<void>((resolve) => {
      release = resolve;
    });
    await previous;
    try {
      return await operation();
    } finally {
      release();
    }
  }
}

import { DurableObject } from "cloudflare:workers";
import type { RankedPool } from "#/matches/schemas.ts";
import {
  expirePendingPairings,
  nextPairingDeadline,
  nextSeekWidening,
  runMatchmakingPass,
} from "./matchmaking.server.ts";

interface MatchmakerIdentity {
  pool: RankedPool;
  season: number;
}

const MAX_PASSES_PER_INVOCATION = 10;

export class MatchmakerDurableObject extends DurableObject<CloudflareBindings> {
  private tail: Promise<void> = Promise.resolve();

  async kick(pool: RankedPool, season: number): Promise<number> {
    return this.serialized(async () => {
      await this.ctx.storage.put<MatchmakerIdentity>("identity", { pool, season });
      const count = await this.drain(pool, season);
      await this.scheduleNextAlarm(pool, season);
      return count;
    });
  }

  async alarm(): Promise<void> {
    await this.serialized(async () => {
      const identity = await this.ctx.storage.get<MatchmakerIdentity>("identity");
      if (!identity) return;
      try {
        await expirePendingPairings(this.env.DB);
        await this.drain(identity.pool, identity.season);
        await this.scheduleNextAlarm(identity.pool, identity.season);
      } catch (error) {
        await this.ctx.storage.setAlarm(Date.now() + 60_000);
        throw error;
      }
    });
  }

  private async scheduleNextAlarm(pool: RankedPool, season: number): Promise<void> {
    const [deadline, widening] = await Promise.all([
      nextPairingDeadline(this.env.DB, pool, season),
      nextSeekWidening(this.env.DB, pool),
    ]);
    const next = [deadline, widening]
      .filter((value): value is number => value !== null)
      .sort((left, right) => left - right)[0];
    if (next === undefined) await this.ctx.storage.deleteAlarm();
    else await this.ctx.storage.setAlarm(Math.max(Date.now(), next));
  }

  private async drain(pool: RankedPool, season: number): Promise<number> {
    let total = 0;
    for (let pass = 0; pass < MAX_PASSES_PER_INVOCATION; pass += 1) {
      const created = await runMatchmakingPass(this.env.DB, pool, season);
      total += created;
      if (created === 0) break;
    }
    return total;
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

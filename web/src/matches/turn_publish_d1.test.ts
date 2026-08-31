import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import { eq } from "drizzle-orm";
import { describe, expect, it } from "vitest";
import { matches } from "#/db/global.ts";

const db = drizzle(env.DB);

/**
 * The turn publish reads the match name out of the same write that sets the
 * turn, because that name is what a notification has to say. This is the check
 * that the driver actually hands it back, which no unit test can stand in for.
 */
function seconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

describe("publishing a turn", () => {
  it("returns the match name from the update that sets the turn", async () => {
    const suffix = crypto.randomUUID().replace(/-/g, "").slice(0, 8);
    const userId = `pub-${suffix}`;
    const matchId = `p${suffix}`;
    const now = new Date("2026-08-28T18:00:00.000Z");
    const settings = JSON.stringify({
      fogEnabled: false,
      startingFunds: 1000,
      hotseatEnabled: false,
      bannedCoIds: [],
      clock: { initialMs: 604_800_000, incrementMs: 172_800_000, maxBankMs: 604_800_000 },
    });

    const mapId = `${suffix}0000`.slice(0, 12);
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
      ).bind(userId, "Publisher", `${userId}@example.com`, seconds(now), seconds(now)),
      env.DB.prepare(
        "INSERT INTO maps (id, name, author, currentRevision, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
      ).bind(mapId, "Amber Valley", "AWBW", seconds(now), seconds(now)),
      env.DB.prepare(`
        INSERT INTO map_revisions
          (mapId, revision, contentHash, width, height, playerCount, propertySignature, unitSignature, createdAt)
        VALUES (?, 1, 'hash', 20, 20, 2, 'p', 'u', ?)
      `).bind(mapId, seconds(now)),
      env.DB.prepare(`
        INSERT INTO matches
          (id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers, isPrivate, joinSlug, settings, createdAt, updatedAt)
        VALUES (?, 'Sand Island', 'active', ?, ?, 1, 2, 0, NULL, ?, ?, ?)
      `).bind(matchId, userId, mapId, settings, seconds(now), seconds(now)),
    ]);

    const rows = await db
      .update(matches)
      .set({ activeSlotIndex: 1, turnDeadlineAt: new Date(now.getTime() + 60_000) })
      .where(eq(matches.id, matchId))
      .returning({ name: matches.name });

    expect(rows).toHaveLength(1);
    expect(rows[0]?.name).toBe("Sand Island");
  });

  it("returns nothing for a match that is not there, rather than throwing", async () => {
    // The publish falls back to a generic name on this, so it must not throw.
    const rows = await db
      .update(matches)
      .set({ activeSlotIndex: null, turnDeadlineAt: null })
      .where(eq(matches.id, "missing12345"))
      .returning({ name: matches.name });

    expect(rows).toHaveLength(0);
  });
});

import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { canonicalizeAwbwMap } from "#/server_wasm.ts";
import amberValley from "../../../assets/maps/61748.json";
import { runMatchmakingPass } from "./matchmaking.server.ts";

const NOW = new Date("2026-08-28T18:00:00.000Z");
const seconds = (date: Date) => Math.floor(date.getTime() / 1000);
const AT = seconds(NOW) - 3600;

/**
 * Amber Valley with `revisions` unranked revisions, the last one current, and
 * revision 1 listed in the async pool.
 */
async function seed(revisions: number): Promise<void> {
  const imported = canonicalizeAwbwMap(awbwMapDataSchema.parse(amberValley));
  const { width, height, terrain, units } = imported.document;
  const hashes = Array.from(
    { length: revisions },
    (_, index) => `${imported.contentHash}-${index}`,
  );
  for (const hash of hashes) {
    await env.CONTENT.put(`maps/doc/v1/${hash}`, JSON.stringify({ width, height, terrain, units }));
  }

  const user = (id: string) =>
    env.DB.prepare(
      "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
    ).bind(id, id, `${id}@example.com`, AT, AT);
  const seek = (id: string) =>
    env.DB.prepare(
      "INSERT INTO seeks (userId, pool, generation, maxActiveMatches, createdAt) VALUES (?, 'async', ?, 3, ?)",
    ).bind(id, `${id}-g`, AT);
  await env.DB.batch([
    env.DB.prepare("DELETE FROM pairings"),
    env.DB.prepare("DELETE FROM match_participants"),
    env.DB.prepare("DELETE FROM matches"),
    env.DB.prepare("DELETE FROM seeks"),
    env.DB.prepare("DELETE FROM ranked_maps"),
    env.DB.prepare("DELETE FROM map_revisions"),
    env.DB.prepare("DELETE FROM maps"),
    env.DB.prepare("DELETE FROM seasons"),
    env.DB.prepare("DELETE FROM user"),
  ]);
  await env.DB.batch([
    user("first"),
    user("second"),
    env.DB.prepare("INSERT INTO seasons (number, startsAt, endsAt) VALUES (1, ?, ?)").bind(
      AT,
      seconds(NOW) + 86_400,
    ),
    env.DB.prepare(
      "INSERT INTO maps (id, name, author, currentRevision, createdAt, updatedAt) VALUES ('amber', 'Amber Valley', 'AWBW', ?, ?, ?)",
    ).bind(revisions, AT, AT),
    ...hashes.map((hash, index) =>
      env.DB.prepare(`
        INSERT INTO map_revisions
          (mapId, revision, contentHash, width, height, playerCount, propertySignature, unitSignature, createdAt)
        VALUES ('amber', ?, ?, ?, ?, 2, 'p', 'u', ?)
      `).bind(index + 1, hash, width, height, AT),
    ),
    env.DB.prepare(
      "INSERT INTO ranked_maps (season, pool, mapId, revision, addedAt) VALUES (1, 'async', 'amber', 1, ?)",
    ).bind(AT),
    seek("first"),
    seek("second"),
  ]);
}

async function pairedRevisions(): Promise<number[]> {
  const result = await env.DB.prepare("SELECT mapRevision FROM matches").all<{
    mapRevision: number;
  }>();
  return result.results.map((row) => row.mapRevision);
}

describe("runMatchmakingPass", () => {
  it("pairs on the revision a pool map was listed with, ranked or not", async () => {
    await seed(1);

    await expect(runMatchmakingPass(env.DB, "async", 1, NOW)).resolves.toBe(1);
    expect(await pairedRevisions()).toEqual([1]);
  });

  it("keeps pairing on the listed revision after the author saves another", async () => {
    await seed(2);

    await expect(runMatchmakingPass(env.DB, "async", 1, NOW)).resolves.toBe(1);
    expect(await pairedRevisions()).toEqual([1]);
  });
});

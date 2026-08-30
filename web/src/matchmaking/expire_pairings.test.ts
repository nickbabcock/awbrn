/**
 * What an unanswered pairing does to the seek behind it, against a real
 * database.
 *
 * A player who lets the confirmation window close has stopped looking. The
 * next pairing would take another opponent out of the pool for 24 hours to
 * reach the same end, so the silent player's seek stops with the window and
 * they start it again themselves. A player who declined made a choice, and
 * keeps their seek.
 */

import { env } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import { expirePendingPairings } from "./matchmaking.server.ts";

const NOW = new Date("2026-08-28T18:00:00.000Z");
const OPENED_AT = new Date(NOW.getTime() - 25 * 60 * 60 * 1000);
const DEADLINE = new Date(NOW.getTime() - 60 * 1000);

function seconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

async function seed(options: { patientIsReady: boolean }): Promise<void> {
  const settings = JSON.stringify({
    fogEnabled: false,
    startingFunds: 1000,
    hotseatEnabled: false,
    bannedCoIds: [],
  });

  await env.DB.batch([
    env.DB.prepare("DELETE FROM pairings"),
    env.DB.prepare("DELETE FROM match_participants"),
    env.DB.prepare("DELETE FROM match_voids"),
    env.DB.prepare("DELETE FROM seeks"),
    env.DB.prepare("DELETE FROM matches"),
    env.DB.prepare("DELETE FROM map_revisions"),
    env.DB.prepare("DELETE FROM maps"),
    env.DB.prepare("DELETE FROM seasons"),
    env.DB.prepare("DELETE FROM user"),
  ]);

  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
    ).bind("ghost", "Ghost", "ghost@example.com", seconds(OPENED_AT), seconds(OPENED_AT)),
    env.DB.prepare(
      "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
    ).bind("patient", "Patient", "patient@example.com", seconds(OPENED_AT), seconds(OPENED_AT)),
    env.DB.prepare("INSERT INTO seasons (number, startsAt, endsAt) VALUES (1, ?, ?)").bind(
      seconds(OPENED_AT),
      seconds(NOW) + 86_400,
    ),
    env.DB.prepare(
      "INSERT INTO maps (id, name, author, currentRevision, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
    ).bind("000000061748", "Amber Valley", "AWBW", seconds(OPENED_AT), seconds(OPENED_AT)),
    env.DB.prepare(`
        INSERT INTO map_revisions
          (mapId, revision, contentHash, width, height, playerCount, propertySignature, unitSignature, createdAt)
        VALUES (?, 1, 'hash', 20, 20, 2, 'p', 'u', ?)
      `).bind("000000061748", seconds(OPENED_AT)),
    env.DB.prepare(`
        INSERT INTO matches
          (id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers, isPrivate, joinSlug, settings, createdAt, updatedAt, pool, season)
        VALUES (?, 'Ranked async', 'pending', 'ghost', ?, 1, 2, 1, NULL, ?, ?, ?, 'async', 1)
      `).bind("match-1", "000000061748", settings, seconds(OPENED_AT), seconds(OPENED_AT)),
    env.DB.prepare(`
        INSERT INTO match_participants (matchId, userId, slotIndex, factionId, coId, ready, joinedAt, updatedAt)
        VALUES (?, 'ghost', 0, 1, NULL, 0, ?, ?)
      `).bind("match-1", seconds(OPENED_AT), seconds(OPENED_AT)),
    env.DB.prepare(`
        INSERT INTO match_participants (matchId, userId, slotIndex, factionId, coId, ready, joinedAt, updatedAt)
        VALUES (?, 'patient', 1, 2, 5, ?, ?, ?)
      `).bind("match-1", options.patientIsReady ? 1 : 0, seconds(OPENED_AT), seconds(OPENED_AT)),
    env.DB.prepare(`
        INSERT INTO pairings
          (id, matchId, pool, season, userOneId, userTwoId, userOneSeekGeneration, userTwoSeekGeneration, status, createdAt, deadlineAt)
        VALUES ('pairing-1', ?, 'async', 1, 'ghost', 'patient', 'g1', 'p1', 'pending', ?, ?)
      `).bind("match-1", seconds(OPENED_AT), seconds(DEADLINE)),
    env.DB.prepare(
      "INSERT INTO seeks (userId, pool, generation, maxActiveMatches, createdAt) VALUES ('ghost', 'async', 'g1', 3, ?)",
    ).bind(seconds(OPENED_AT)),
    env.DB.prepare(
      "INSERT INTO seeks (userId, pool, generation, maxActiveMatches, createdAt) VALUES ('patient', 'async', 'p1', 3, ?)",
    ).bind(seconds(OPENED_AT)),
  ]);
}

async function remainingSeeks(): Promise<string[]> {
  const result = await env.DB.prepare("SELECT userId FROM seeks ORDER BY userId").all<{
    userId: string;
  }>();
  return result.results.map((row) => row.userId);
}

describe("expirePendingPairings", () => {
  beforeEach(async () => {
    await seed({ patientIsReady: true });
  });

  it("stops the seek of the player who never answered", async () => {
    await expirePendingPairings(env.DB, NOW);

    expect(await remainingSeeks()).toEqual(["patient"]);
  });

  it("stops both seeks when neither player answered", async () => {
    await seed({ patientIsReady: false });

    await expirePendingPairings(env.DB, NOW);

    expect(await remainingSeeks()).toEqual([]);
  });

  it("voids the match and records the expiry", async () => {
    const expired = await expirePendingPairings(env.DB, NOW);

    expect(expired).toBe(1);
    const match = await env.DB.prepare("SELECT phase FROM matches WHERE id = 'match-1'").first<{
      phase: string;
    }>();
    const pairing = await env.DB.prepare(
      "SELECT status FROM pairings WHERE id = 'pairing-1'",
    ).first<{ status: string }>();
    const voided = await env.DB.prepare(
      "SELECT publicReason FROM match_voids WHERE matchId = 'match-1'",
    ).first<{ publicReason: string }>();

    expect(match?.phase).toBe("cancelled");
    expect(pairing?.status).toBe("expired");
    expect(voided?.publicReason).toBe("Ranked confirmation expired");
  });

  it("leaves a window that is still open alone", async () => {
    const beforeDeadline = new Date(DEADLINE.getTime() - 60 * 1000);

    const expired = await expirePendingPairings(env.DB, beforeDeadline);

    expect(expired).toBe(0);
    expect(await remainingSeeks()).toEqual(["ghost", "patient"]);
  });
});

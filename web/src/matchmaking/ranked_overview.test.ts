/**
 * What the ranked hub reads, against a real database.
 *
 * The hub describes the viewer and nothing else, so the checks here watch the
 * two places that rule is easy to break: the opponent stays hidden while a
 * pairing waits for confirmation, and the standings order players by what is
 * known about them rather than by the rating alone.
 */

import { env } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import { rankedOverview, rankedStandings } from "./ranked_overview.server.ts";

const NOW = new Date("2026-08-28T18:00:00.000Z");
const OPENED_AT = new Date(NOW.getTime() - 3 * 60 * 60 * 1000);
const DEADLINE = new Date(NOW.getTime() + 21 * 60 * 60 * 1000);

function seconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

function insertUser(id: string, name: string): D1PreparedStatement {
  return env.DB.prepare(
    "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
  ).bind(id, name, `${id}@example.com`, seconds(OPENED_AT), seconds(OPENED_AT));
}

function insertMatch(id: string, phase: string): D1PreparedStatement {
  const settings = JSON.stringify({
    fogEnabled: false,
    startingFunds: 1000,
    hotseatEnabled: false,
    bannedCoIds: [],
  });
  return env.DB.prepare(`
      INSERT INTO matches
        (id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers, isPrivate, joinSlug, settings, createdAt, updatedAt, pool, season)
      VALUES (?, 'Ranked async', ?, 'viewer', '000000061748', 1, 2, 1, NULL, ?, ?, ?, 'async', 1)
    `).bind(id, phase, settings, seconds(OPENED_AT), seconds(OPENED_AT));
}

function insertSeat(
  matchId: string,
  userId: string,
  slotIndex: number,
  options: { coId: number | null; ready: boolean },
): D1PreparedStatement {
  return env.DB.prepare(`
      INSERT INTO match_participants (matchId, userId, slotIndex, factionId, coId, ready, joinedAt, updatedAt)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `).bind(
    matchId,
    userId,
    slotIndex,
    slotIndex + 1,
    options.coId,
    options.ready ? 1 : 0,
    seconds(OPENED_AT),
    seconds(OPENED_AT),
  );
}

beforeEach(async () => {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM pairings"),
    env.DB.prepare("DELETE FROM match_participants"),
    env.DB.prepare("DELETE FROM ratings"),
    env.DB.prepare("DELETE FROM seeks"),
    env.DB.prepare("DELETE FROM matches"),
    env.DB.prepare("DELETE FROM map_revisions"),
    env.DB.prepare("DELETE FROM maps"),
    env.DB.prepare("DELETE FROM seasons"),
    env.DB.prepare("DELETE FROM user"),
  ]);

  await env.DB.batch([
    insertUser("viewer", "Viewer"),
    insertUser("rival", "Rival"),
    insertUser("newcomer", "Newcomer"),
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
        VALUES ('000000061748', 1, 'hash', 20, 20, 2, 'p', 'u', ?)
      `).bind(seconds(OPENED_AT)),
  ]);
});

describe("rankedOverview", () => {
  it("holds the opponent back while the pairing waits, and names them in play", async () => {
    await env.DB.batch([
      insertMatch("match-pending", "pending"),
      insertSeat("match-pending", "viewer", 0, { coId: 5, ready: true }),
      insertSeat("match-pending", "rival", 1, { coId: null, ready: false }),
      env.DB.prepare(`
          INSERT INTO pairings
            (id, matchId, pool, season, userOneId, userTwoId, userOneSeekGeneration, userTwoSeekGeneration, status, createdAt, deadlineAt)
          VALUES ('pairing-1', 'match-pending', 'async', 1, 'rival', 'viewer', 'r1', 'v1', 'pending', ?, ?)
        `).bind(seconds(OPENED_AT), seconds(DEADLINE)),
      insertMatch("match-active", "active"),
      insertSeat("match-active", "viewer", 0, { coId: 9, ready: true }),
      insertSeat("match-active", "rival", 1, { coId: 3, ready: true }),
    ]);

    const overview = await rankedOverview("viewer", NOW);
    const async = overview.pools.find((pool) => pool.pool === "async");

    expect(async?.activeMatches).toBe(2);
    expect(async?.pending).toEqual([
      {
        matchId: "match-pending",
        mapName: "Amber Valley",
        mapId: "000000061748",
        mapRevision: 1,
        slotIndex: 0,
        factionId: 1,
        hasCommander: true,
        isReady: true,
        deadlineAt: DEADLINE.toISOString(),
      },
    ]);
    expect(async?.inPlay).toEqual([
      {
        matchId: "match-active",
        mapName: "Amber Valley",
        mapId: "000000061748",
        mapRevision: 1,
        slotIndex: 0,
        factionId: 1,
        coId: 9,
        opponentName: "Rival",
        updatedAt: OPENED_AT.toISOString(),
      },
    ]);
  });

  it("reads the seek and the rating for the pool", async () => {
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO seeks (userId, pool, generation, maxActiveMatches, createdAt) VALUES ('viewer', 'async', 'v1', 4, ?)",
      ).bind(seconds(OPENED_AT)),
      env.DB.prepare(`
          INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches)
          VALUES ('viewer', 'async', 1620, 60, 0.06, ?, 12)
        `).bind(seconds(OPENED_AT)),
    ]);

    const overview = await rankedOverview("viewer", NOW);
    const async = overview.pools.find((pool) => pool.pool === "async");

    expect(overview.isEmailVerified).toBe(true);
    expect(overview.season?.number).toBe(1);
    expect(async?.seek).toEqual({ maxActiveMatches: 4, createdAt: OPENED_AT.toISOString() });
    // Three hours is short of one complete inactive period, so the stored
    // deviation stands and the rating is out of the provisional range.
    expect(async?.rating).toEqual({
      rating: 1620,
      deviation: 60,
      ratedMatches: 12,
      isProvisional: false,
    });
  });

  it("reports no seek and no rating for a player who has neither", async () => {
    const overview = await rankedOverview("newcomer", NOW);
    expect(overview.pools.map((pool) => pool.pool)).toEqual([
      "async",
      "fog_async",
      "live",
      "fog_live",
    ]);
    expect(overview.pools.every((pool) => pool.seek === null && pool.rating === null)).toBe(true);
  });
});

describe("rankedStandings", () => {
  beforeEach(async () => {
    await env.DB.batch([
      env.DB.prepare(`
          INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches)
          VALUES ('viewer', 'async', 1620, 60, 0.06, ?, 12)
        `).bind(seconds(OPENED_AT)),
      env.DB.prepare(`
          INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches)
          VALUES ('rival', 'async', 1700, 55, 0.06, ?, 30)
        `).bind(seconds(OPENED_AT)),
      // Above the provisional bound, so the rating still reads with a
      // question mark, but well inside the bound which holds a ladder place.
      env.DB.prepare(`
          INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches)
          VALUES ('newcomer', 'async', 1540, 200, 0.06, ?, 2)
        `).bind(seconds(OPENED_AT)),
    ]);
  });

  it("orders the ladder by the rating, less what is not known about it", async () => {
    const standings = await rankedStandings("async", "newcomer", NOW);

    // The newcomer is 80 points behind the viewer on rating and 140 behind
    // once the deviation is counted, so an unsure rating sits below a settled
    // one rather than being left out of the ladder.
    expect(standings.entries).toEqual([
      { rank: 1, userId: "rival", name: "Rival", rating: 1700, ratedMatches: 30, isViewer: false },
      {
        rank: 2,
        userId: "viewer",
        name: "Viewer",
        rating: 1620,
        ratedMatches: 12,
        isViewer: false,
      },
      {
        rank: 3,
        userId: "newcomer",
        name: "Newcomer",
        rating: 1540,
        ratedMatches: 2,
        isViewer: true,
      },
    ]);
    expect(standings.viewer).toBeNull();
  });

  it("leaves out a rating older than a season, and reports it to its owner", async () => {
    await env.DB.prepare("UPDATE ratings SET lastRatedAt = ? WHERE userId = 'newcomer'")
      .bind(seconds(new Date(NOW.getTime() - 120 * 24 * 60 * 60 * 1000)))
      .run();

    const standings = await rankedStandings("async", "newcomer", NOW);

    expect(standings.entries.map((entry) => entry.userId)).toEqual(["rival", "viewer"]);
    expect(standings.viewer).toEqual({ rating: 1540, ratedMatches: 2, isProvisional: true });
  });

  it("marks the viewer in the list", async () => {
    const standings = await rankedStandings("async", "viewer", NOW);
    expect(standings.entries.map((entry) => entry.isViewer)).toEqual([false, true, false]);
    expect(standings.viewer).toBeNull();
  });

  it("stops the deviation growth while a rated match is under way", async () => {
    // Far enough back that the growth alone would push the rating out of the
    // standings; the match in play holds the deviation where it is.
    await env.DB.batch([
      env.DB.prepare(
        "UPDATE ratings SET deviation = 140, lastRatedAt = ? WHERE userId = 'viewer'",
      ).bind(seconds(new Date(NOW.getTime() - 200 * 24 * 60 * 60 * 1000))),
      insertMatch("match-active", "active"),
      insertSeat("match-active", "viewer", 0, { coId: 9, ready: true }),
      insertSeat("match-active", "rival", 1, { coId: 3, ready: true }),
    ]);

    const standings = await rankedStandings("async", "viewer", NOW);
    expect(standings.entries.map((entry) => entry.userId)).toEqual(["rival", "viewer", "newcomer"]);
  });
});

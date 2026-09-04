/**
 * The rating pass, against a real database.
 *
 * The checks here watch the three things which make the pass safe to wake more
 * than once: a result is rated one time, a result nobody can rate is stamped
 * and left, and a voided result never reaches a rating at all.
 */

import { env } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import { applyPendingRatings, captureClosedSeasons, readLadder } from "./ratings.server.ts";

const NOW = new Date("2026-08-28T18:00:00.000Z");
const OPENED_AT = new Date(NOW.getTime() - 3 * 60 * 60 * 1000);

function seconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

function insertUser(id: string, name: string): D1PreparedStatement {
  return env.DB.prepare(
    "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
  ).bind(id, name, `${id}@example.com`, seconds(OPENED_AT), seconds(OPENED_AT));
}

function insertMatch(id: string, phase = "completed", season = 1): D1PreparedStatement {
  const settings = JSON.stringify({
    fogEnabled: false,
    startingFunds: 1000,
    hotseatEnabled: false,
    bannedCoIds: [],
  });
  return env.DB.prepare(`
      INSERT INTO matches
        (id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers, isPrivate, joinSlug,
         settings, createdAt, updatedAt, completedAt, pool, season)
      VALUES (?, 'Ranked async', ?, 'alpha', '000000061748', 1, 2, 1, NULL, ?, ?, ?, ?, 'async', ?)
    `).bind(
    id,
    phase,
    settings,
    seconds(OPENED_AT),
    seconds(OPENED_AT),
    phase === "completed" ? seconds(NOW) : null,
    season,
  );
}

/** One finished seat. `outcome` drives the score the pass reads. */
function insertResult(
  matchId: string,
  slotIndex: number,
  userId: string | null,
  outcome: "win" | "loss" | "draw",
  options: { pool?: string | null; aiProfileId?: string | null; recordedAt?: Date } = {},
): D1PreparedStatement {
  const pool = options.pool === undefined ? "async" : options.pool;
  return env.DB.prepare(`
      INSERT INTO match_results
        (matchId, slotIndex, userId, aiProfileId, teamId, outcome, placement, reason, pool, recordedAt)
      VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)
    `).bind(
    matchId,
    slotIndex,
    userId,
    options.aiProfileId ?? null,
    outcome,
    outcome === "loss" ? 2 : 1,
    outcome === "win" ? null : "rout",
    pool,
    seconds(options.recordedAt ?? NOW),
  );
}

async function ratingOf(userId: string) {
  return env.DB.prepare("SELECT rating, deviation, ratedMatches FROM ratings WHERE userId = ?")
    .bind(userId)
    .first<{ rating: number; deviation: number; ratedMatches: number }>();
}

beforeEach(async () => {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM season_standings"),
    env.DB.prepare("DELETE FROM season_captures"),
    env.DB.prepare("DELETE FROM rating_updates"),
    env.DB.prepare("DELETE FROM match_voids"),
    env.DB.prepare("DELETE FROM match_results"),
    env.DB.prepare("DELETE FROM match_participants"),
    env.DB.prepare("DELETE FROM ratings"),
    env.DB.prepare("DELETE FROM matches"),
    env.DB.prepare("DELETE FROM map_revisions"),
    env.DB.prepare("DELETE FROM maps"),
    env.DB.prepare("DELETE FROM seasons"),
    env.DB.prepare("DELETE FROM user"),
  ]);

  await env.DB.batch([
    insertUser("alpha", "Alpha"),
    insertUser("bravo", "Bravo"),
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

describe("applyPendingRatings", () => {
  it("moves the winner up and the loser down, and stamps the result", async () => {
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
    ]);

    const { applied, drained } = await applyPendingRatings(env.DB, "async", NOW);

    expect(drained).toBe(true);
    expect(applied).toHaveLength(2);

    const winner = await ratingOf("alpha");
    const loser = await ratingOf("bravo");
    expect(winner!.rating).toBeGreaterThan(1500);
    expect(loser!.rating).toBeLessThan(1500);
    expect(winner!.ratedMatches).toBe(1);
    // A first match settles the rating well below the unrated deviation.
    expect(winner!.deviation).toBeLessThan(350);

    const stamped = await env.DB.prepare(
      "SELECT count(*) AS n FROM match_results WHERE ratedAt IS NOT NULL",
    ).first<{ n: number }>();
    expect(stamped!.n).toBe(2);
  });

  it("rates one result once, however many times the pass is run", async () => {
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
    ]);

    await applyPendingRatings(env.DB, "async", NOW);
    const afterFirst = await ratingOf("alpha");

    const second = await applyPendingRatings(env.DB, "async", NOW);
    const afterSecond = await ratingOf("alpha");

    expect(second.applied).toHaveLength(0);
    expect(afterSecond).toEqual(afterFirst);
    expect(afterSecond!.ratedMatches).toBe(1);
  });

  it("writes what each match did, so a report can read it back", async () => {
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
    ]);

    await applyPendingRatings(env.DB, "async", NOW);

    const row = await env.DB.prepare(
      "SELECT ratingBefore, ratingAfter, opponentRating, score, season FROM rating_updates WHERE userId = 'alpha'",
    ).first<{
      ratingBefore: number;
      ratingAfter: number;
      opponentRating: number;
      score: number;
      season: number;
    }>();

    expect(row!.ratingBefore).toBe(1500);
    expect(row!.ratingAfter).toBeGreaterThan(1500);
    expect(row!.opponentRating).toBe(1500);
    expect(row!.score).toBe(1);
    expect(row!.season).toBe(1);
  });

  it("rates two matches of one player in the order they were played", async () => {
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
      insertMatch("match-2"),
      insertResult("match-2", 0, "alpha", "win"),
      insertResult("match-2", 1, "bravo", "loss"),
    ]);

    await applyPendingRatings(env.DB, "async", NOW);

    const alpha = await ratingOf("alpha");
    expect(alpha!.ratedMatches).toBe(2);

    const updates = await env.DB.prepare(
      "SELECT matchId, ratingBefore, ratingAfter FROM rating_updates WHERE userId = 'alpha' ORDER BY matchId",
    ).all<{ matchId: string; ratingBefore: number; ratingAfter: number }>();

    // The second match starts from what the first one left.
    expect(updates.results[0]!.ratingBefore).toBe(1500);
    expect(updates.results[1]!.ratingBefore).toBeCloseTo(updates.results[0]!.ratingAfter, 6);
    expect(alpha!.rating).toBeCloseTo(updates.results[1]!.ratingAfter, 6);
  });

  it("leaves a voided match out, and stops reading it again", async () => {
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
      env.DB.prepare(
        "INSERT INTO match_voids (matchId, publicReason, voidedAt) VALUES ('match-1', 'testing', ?)",
      ).bind(seconds(NOW)),
    ]);

    const { applied } = await applyPendingRatings(env.DB, "async", NOW);

    expect(applied).toHaveLength(0);
    expect(await ratingOf("alpha")).toBeNull();

    const stamped = await env.DB.prepare(
      "SELECT count(*) AS n FROM match_results WHERE ratedAt IS NOT NULL",
    ).first<{ n: number }>();
    expect(stamped!.n).toBe(2);
  });

  it("never reads a match the server took a seat in", async () => {
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win", { pool: null }),
      insertResult("match-1", 1, null, "loss", { pool: null, aiProfileId: "ai-hard-v1" }),
    ]);

    const { applied } = await applyPendingRatings(env.DB, "async", NOW);

    expect(applied).toHaveLength(0);
    expect(await ratingOf("alpha")).toBeNull();
  });

  it("reads the queue by match, so a batch cannot cut a match between its seats", async () => {
    // One more match than a batch holds, so the pass stops on a boundary. The
    // batch counts matches, and a match which is cut in two reads as one seat,
    // which is stamped as unratable and never rated.
    const statements: D1PreparedStatement[] = [];
    for (let index = 0; index < 51; index += 1) {
      const id = `match-${String(index).padStart(3, "0")}`;
      statements.push(
        insertMatch(id),
        insertResult(id, 0, "alpha", "win"),
        insertResult(id, 1, "bravo", "loss"),
      );
    }
    await env.DB.batch(statements);

    const first = await applyPendingRatings(env.DB, "async", NOW);
    expect(first.drained).toBe(false);
    const second = await applyPendingRatings(env.DB, "async", NOW);
    expect(second.drained).toBe(true);

    // Nothing was stamped without being rated, and no seat was left behind.
    const stray = await env.DB.prepare(`
        SELECT count(*) AS n FROM match_results
        WHERE matchId NOT IN (SELECT matchId FROM rating_updates) OR ratedAt IS NULL
      `).first<{ n: number }>();
    expect(stray!.n).toBe(0);
  });

  it("stamps a result it cannot rate rather than reading it on every wake", async () => {
    // One seat, which is a match with nobody to rate it against.
    await env.DB.batch([insertMatch("match-1"), insertResult("match-1", 0, "alpha", "win")]);

    const { applied } = await applyPendingRatings(env.DB, "async", NOW);

    expect(applied).toHaveLength(0);
    const stamped = await env.DB.prepare(
      "SELECT ratedAt FROM match_results WHERE matchId = 'match-1'",
    ).first<{ ratedAt: number | null }>();
    expect(stamped!.ratedAt).not.toBeNull();
  });
});

describe("readLadder", () => {
  it("puts a rating nobody has tested lately below an active one it beats", async () => {
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches) VALUES ('alpha', 'async', 1720, 50, 0.06, ?, 30)",
      ).bind(seconds(new Date(NOW.getTime() - 70 * 24 * 60 * 60 * 1000))),
      env.DB.prepare(
        "INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches) VALUES ('bravo', 'async', 1700, 50, 0.06, ?, 30)",
      ).bind(seconds(NOW)),
    ]);

    const ladder = await readLadder(env.DB, "async", NOW);

    expect(ladder.map((row) => row.userId)).toEqual(["bravo", "alpha"]);
    // The idle player keeps a place, and the deviation reports why they slid.
    expect(ladder[1]!.deviation).toBeGreaterThan(50);
  });

  it("keeps a player far above the field at the top while they slide", async () => {
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches) VALUES ('alpha', 'async', 2100, 50, 0.06, ?, 30)",
      ).bind(seconds(new Date(NOW.getTime() - 70 * 24 * 60 * 60 * 1000))),
      env.DB.prepare(
        "INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches) VALUES ('bravo', 'async', 1700, 50, 0.06, ?, 30)",
      ).bind(seconds(NOW)),
    ]);

    const ladder = await readLadder(env.DB, "async", NOW);
    expect(ladder.map((row) => row.userId)).toEqual(["alpha", "bravo"]);
  });

  it("drops a rating which is older than a season", async () => {
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches) VALUES ('alpha', 'async', 1900, 50, 0.06, ?, 30)",
      ).bind(seconds(new Date(NOW.getTime() - 120 * 24 * 60 * 60 * 1000))),
    ]);

    expect(await readLadder(env.DB, "async", NOW)).toEqual([]);
  });
});

/** Move the season's end into the past, so it is ready to be frozen. */
async function closeSeason(): Promise<void> {
  await env.DB.prepare("UPDATE seasons SET endsAt = ? WHERE number = 1")
    .bind(seconds(NOW) - 3600)
    .run();
}

/** A rating which has been played enough to hold a ladder place. */
function settledRating(userId: string, rating: number): D1PreparedStatement {
  return env.DB.prepare(`
      INSERT INTO ratings (userId, pool, rating, deviation, volatility, lastRatedAt, ratedMatches)
      VALUES (?, 'async', ?, 60, 0.06, ?, 20)
    `).bind(userId, rating, seconds(NOW));
}

describe("captureClosedSeasons", () => {
  it("records the freeze of a season nobody holds a place in", async () => {
    await closeSeason();

    expect(await captureClosedSeasons(env.DB, "async", NOW)).toEqual([1]);
    // The freeze happened, so it is not worked out again on the next wake.
    expect(await captureClosedSeasons(env.DB, "async", NOW)).toEqual([]);
  });

  it("waits for a season whose matches are still being played", async () => {
    await closeSeason();
    await env.DB.batch([insertMatch("match-live", "active")]);

    expect(await captureClosedSeasons(env.DB, "async", NOW)).toEqual([]);
  });

  it("waits for a result which has not been rated", async () => {
    await closeSeason();
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
    ]);

    expect(await captureClosedSeasons(env.DB, "async", NOW)).toEqual([]);
  });

  it("freezes the ladder once the season is finished, and freezes it once", async () => {
    await closeSeason();
    // Both players arrive with a settled rating, which one match cannot give:
    // a first match leaves the deviation near 290, above the ladder limit.
    await env.DB.batch([settledRating("alpha", 1700), settledRating("bravo", 1600)]);
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
    ]);
    await applyPendingRatings(env.DB, "async", NOW);

    expect(await captureClosedSeasons(env.DB, "async", NOW)).toEqual([1]);

    const frozen = await env.DB.prepare(
      "SELECT rank, userId, name, rating, deviation, ratedMatches, seasonMatches FROM season_standings ORDER BY rank",
    ).all<{
      rank: number;
      userId: string;
      name: string;
      rating: number;
      deviation: number;
      ratedMatches: number;
      seasonMatches: number;
    }>();

    expect(frozen.results.map((row) => row.userId)).toEqual(["alpha", "bravo"]);
    expect(frozen.results[0]!.rank).toBe(1);
    expect(frozen.results[0]!.name).toBe("Alpha");
    expect(frozen.results[0]!.seasonMatches).toBe(1);
    expect(frozen.results[0]!.ratedMatches).toBe(21);

    // A season is frozen once. A later pass finds nothing left to freeze.
    expect(await captureClosedSeasons(env.DB, "async", NOW)).toEqual([]);
  });

  it("freezes the season which closed, not the ladder the next season moved", async () => {
    await closeSeason();
    await env.DB.prepare("INSERT INTO seasons (number, startsAt, endsAt) VALUES (2, ?, ?)")
      .bind(seconds(NOW) - 3600, seconds(NOW) + 86_400)
      .run();
    await env.DB.batch([settledRating("alpha", 1700), settledRating("bravo", 1600)]);

    // The last match of season 1 and a match of season 2 are rated together,
    // which is what happens when an async match runs past the season's end.
    const later = new Date(NOW.getTime() + 60 * 60 * 1000);
    await env.DB.batch([
      insertMatch("match-1", "completed", 1),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
      insertMatch("match-2", "completed", 2),
      insertResult("match-2", 0, "bravo", "win", { recordedAt: later }),
      insertResult("match-2", 1, "alpha", "loss", { recordedAt: later }),
    ]);
    await applyPendingRatings(env.DB, "async", NOW);

    expect(await captureClosedSeasons(env.DB, "async", NOW)).toEqual([1]);

    const seasonOne = await env.DB.prepare(
      "SELECT ratingAfter FROM rating_updates WHERE matchId = 'match-1' AND userId = 'alpha'",
    ).first<{ ratingAfter: number }>();
    const frozen = await env.DB.prepare(
      "SELECT rank, userId, rating, ratedMatches, seasonMatches FROM season_standings ORDER BY rank",
    ).all<{
      rank: number;
      userId: string;
      rating: number;
      ratedMatches: number;
      seasonMatches: number;
    }>();

    expect(frozen.results.map((row) => row.userId)).toEqual(["alpha", "bravo"]);
    // The rating season 1 ended on, before the season 2 loss took it back.
    expect(frozen.results[0]!.rating).toBeCloseTo(seasonOne!.ratingAfter, 6);
    expect(frozen.results[0]!.ratedMatches).toBe(21);
    expect(frozen.results[0]!.seasonMatches).toBe(1);
    expect((await ratingOf("alpha"))!.rating).toBeLessThan(frozen.results[0]!.rating);
  });

  it("freezes the rank and the grown deviation, so a later read cannot empty it", async () => {
    await closeSeason();
    await env.DB.batch([settledRating("alpha", 1700), settledRating("bravo", 1600)]);
    await env.DB.batch([
      insertMatch("match-1"),
      insertResult("match-1", 0, "alpha", "win"),
      insertResult("match-1", 1, "bravo", "loss"),
    ]);
    await applyPendingRatings(env.DB, "async", NOW);
    await captureClosedSeasons(env.DB, "async", NOW);

    const frozen = await env.DB.prepare(
      "SELECT count(*) AS n FROM season_standings WHERE season = 1",
    ).first<{ n: number }>();
    const muchLater = new Date(NOW.getTime() + 400 * 24 * 60 * 60 * 1000);

    // The live ladder has let both go by now. The frozen rows have not moved.
    expect(await readLadder(env.DB, "async", muchLater)).toEqual([]);
    expect(frozen!.n).toBe(2);
  });
});

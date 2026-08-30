import { and, eq, gt, lte, sql } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { env } from "cloudflare:workers";
import { isKnownCoId } from "#/co_roster.ts";
import {
  maps,
  matchParticipants,
  matches,
  pairings,
  rankedMaps,
  ratings,
  seasons,
  seeks,
  user,
} from "#/db/global.ts";
import type { RawCol, RawRow } from "#/db/raw_sql.ts";
import { loadMapRevision, mapSlotFactionIds } from "#/maps/maps.server.ts";
import { generateMatchId } from "#/matches/match_id.ts";
import { defaultMatchClock } from "#/matches/schemas.ts";
import type { RankedPool } from "#/matches/schemas.ts";
import type { RankedConfirmationRequest } from "#/matches/schemas.ts";
import { finalizeStartingMatchIfNeeded } from "#/matches/matches.server.ts";
import { nanoid } from "#/vendor/nanoid.ts";
import {
  DEFAULT_MAX_ACTIVE_MATCHES,
  HARD_MAX_ACTIVE_MATCHES,
  INITIAL_DEVIATION,
  INITIAL_RATING,
  selectMatchmakingPairs,
  userPairKey,
  type MatchmakingCandidate,
} from "./matchmaking.ts";
import { getMatchmakerStub } from "./matchmaker_service.ts";

const CANDIDATE_LIMIT = 200;
const CONFIRMATION_WINDOW_MS = 24 * 60 * 60 * 1000;

type SeekColumns = typeof seeks.$inferSelect;
type RatingColumns = typeof ratings.$inferSelect;
type PairingColumns = typeof pairings.$inferSelect;

interface CandidateRow extends RawRow<
  SeekColumns,
  "userId" | "pool" | "generation" | "createdAt" | "maxActiveMatches"
> {
  /** Counted by the query, not stored. */
  activeMatches: number;
  /** Null until the user has a rating in the pool, because of the outer join. */
  rating: RawCol<RatingColumns, "rating"> | null;
  deviation: RawCol<RatingColumns, "deviation"> | null;
}

type ActivePairRow = RawRow<PairingColumns, "userOneId" | "userTwoId">;

interface RankedMapRow extends RawRow<typeof rankedMaps.$inferSelect, "mapId"> {
  /** `maps.currentRevision`, under the name the query gives it. */
  revision: RawCol<typeof maps.$inferSelect, "currentRevision">;
}

export interface SeekSnapshot {
  pool: RankedPool;
  maxActiveMatches: number;
  createdAt: string;
}

export async function activeSeasonNumber(now = new Date()): Promise<number | null> {
  const db = drizzle(env.DB, { schema: { seasons } });
  const row = await db
    .select({ number: seasons.number })
    .from(seasons)
    .where(and(lte(seasons.startsAt, now), gt(seasons.endsAt, now)))
    .orderBy(seasons.number)
    .get();
  return row?.number ?? null;
}

export async function startSeek(
  userId: string,
  pool: RankedPool,
  maxActiveMatches = DEFAULT_MAX_ACTIVE_MATCHES,
): Promise<SeekSnapshot> {
  if (pool !== "async") throw new Error("this ranked pool is not open");
  if (
    !Number.isSafeInteger(maxActiveMatches) ||
    maxActiveMatches < 1 ||
    maxActiveMatches > HARD_MAX_ACTIVE_MATCHES
  ) {
    throw new Error(`maximum active matches must be between 1 and ${HARD_MAX_ACTIVE_MATCHES}`);
  }

  const db = drizzle(env.DB, { schema: { seeks, user } });
  const account = await db
    .select({ emailVerified: user.emailVerified })
    .from(user)
    .where(eq(user.id, userId))
    .get();
  if (!account?.emailVerified)
    throw new Error("verify your email address before seeking a ranked match");

  const now = new Date();
  await db
    .insert(seeks)
    .values({ userId, pool, generation: nanoid(), maxActiveMatches, createdAt: now })
    .onConflictDoUpdate({
      target: [seeks.userId, seeks.pool],
      set: { maxActiveMatches },
    })
    .run();

  const row = await db
    .select({
      pool: seeks.pool,
      maxActiveMatches: seeks.maxActiveMatches,
      createdAt: seeks.createdAt,
    })
    .from(seeks)
    .where(and(eq(seeks.userId, userId), eq(seeks.pool, pool)))
    .get();
  if (!row) throw new Error("failed to save ranked seek");

  const season = await activeSeasonNumber(now);
  if (season !== null) await getMatchmakerStub(env.MATCHMAKERS, season, pool).kick(pool, season);
  return { ...row, createdAt: row.createdAt.toISOString() };
}

export async function stopSeek(userId: string, pool: RankedPool): Promise<void> {
  const db = drizzle(env.DB, { schema: { seeks } });
  await db
    .delete(seeks)
    .where(and(eq(seeks.userId, userId), eq(seeks.pool, pool)))
    .run();
}

export async function listSeeks(userId: string): Promise<SeekSnapshot[]> {
  const db = drizzle(env.DB, { schema: { seeks } });
  const rows = await db
    .select({
      pool: seeks.pool,
      maxActiveMatches: seeks.maxActiveMatches,
      createdAt: seeks.createdAt,
    })
    .from(seeks)
    .where(eq(seeks.userId, userId))
    .all();
  return rows.map((row) => ({ ...row, createdAt: row.createdAt.toISOString() }));
}

export async function updateRankedConfirmation(
  matchId: string,
  userId: string,
  action: RankedConfirmationRequest,
): Promise<void> {
  const db = drizzle(env.DB, {
    schema: { matchParticipants, matches, pairings },
  });
  const participant = await db
    .select({
      coId: matchParticipants.coId,
      phase: matches.phase,
      pool: matches.pool,
      season: matches.season,
      bannedCoIds: matches.settings,
    })
    .from(matchParticipants)
    .innerJoin(matches, eq(matches.id, matchParticipants.matchId))
    .where(and(eq(matchParticipants.matchId, matchId), eq(matchParticipants.userId, userId)))
    .get();
  if (!participant || participant.phase !== "pending" || participant.pool === null) {
    throw new Error("ranked match is not awaiting your confirmation");
  }

  const now = new Date();
  switch (action.action) {
    case "selectCommander": {
      if (!isAllowedRankedCo(action.coId)) throw new Error("select an allowed commander");
      const settings =
        typeof participant.bannedCoIds === "string"
          ? (JSON.parse(participant.bannedCoIds) as { bannedCoIds?: unknown })
          : (participant.bannedCoIds as { bannedCoIds?: unknown });
      if (Array.isArray(settings.bannedCoIds) && settings.bannedCoIds.includes(action.coId)) {
        throw new Error("select an allowed commander");
      }
      await db
        .update(matchParticipants)
        .set({ coId: action.coId, ready: false, updatedAt: now })
        .where(and(eq(matchParticipants.matchId, matchId), eq(matchParticipants.userId, userId)))
        .run();
      return;
    }
    case "ready": {
      if (participant.coId === null) {
        throw new Error("select a commander before becoming ready");
      }
      await db.batch([
        db
          .update(matchParticipants)
          .set({ ready: true, updatedAt: now })
          .where(and(eq(matchParticipants.matchId, matchId), eq(matchParticipants.userId, userId))),
        db
          .update(matches)
          .set({ phase: "starting", updatedAt: now })
          .where(
            and(
              eq(matches.id, matchId),
              eq(matches.phase, "pending"),
              sql`(SELECT COUNT(*) FROM match_participants p WHERE p.matchId = ${matchId} AND p.ready = 1 AND p.coId IS NOT NULL) = 2`,
            ),
          ),
        db
          .update(pairings)
          .set({ status: "confirmed", resolvedAt: now })
          .where(
            and(
              eq(pairings.matchId, matchId),
              eq(pairings.status, "pending"),
              sql`EXISTS (SELECT 1 FROM matches m WHERE m.id = ${matchId} AND m.phase = 'starting')`,
            ),
          ),
      ]);
      const pairing = await db
        .select({ status: pairings.status })
        .from(pairings)
        .where(eq(pairings.matchId, matchId))
        .get();
      // A pending pairing means only this player is ready, which is a good first response.
      if (pairing?.status !== "confirmed" && pairing?.status !== "pending") {
        throw new Error("the ranked confirmation window has ended");
      }
      const result = await finalizeStartingMatchIfNeeded(matchId);
      if (!result.ok) throw new Error(result.error.message);
      return;
    }
    case "refuse": {
      const seconds = Math.floor(now.getTime() / 1000);
      const results = await env.DB.batch([
        env.DB.prepare(`
            INSERT INTO match_voids (matchId, publicReason, voidedAt)
            SELECT id, 'A player declined ranked confirmation', ?
            FROM matches WHERE id = ? AND phase = 'pending'
            ON CONFLICT(matchId) DO NOTHING
          `).bind(seconds, matchId),
        env.DB.prepare(
          "UPDATE matches SET phase = 'cancelled', completedAt = ?, updatedAt = ? WHERE id = ? AND phase = 'pending'",
        ).bind(seconds, seconds, matchId),
        env.DB.prepare(
          "UPDATE pairings SET status = 'refused', resolvedAt = ? WHERE matchId = ? AND status = 'pending'",
        ).bind(seconds, matchId),
      ]);
      if ((results[1]?.meta.changes ?? 0) !== 1) {
        throw new Error("the ranked confirmation window has ended");
      }
      if (participant.pool !== null && participant.season !== null) {
        await getMatchmakerStub(env.MATCHMAKERS, participant.season, participant.pool).kick(
          participant.pool,
          participant.season,
        );
      }
    }
  }
}

/** Run one bounded pairing pass for one pool and season. */
export async function runMatchmakingPass(
  database: D1Database,
  pool: RankedPool,
  season: number,
  now = new Date(),
): Promise<number> {
  const candidateResult = await database
    .prepare(`
      SELECT
        s.userId,
        s.pool,
        s.generation,
        s.createdAt,
        s.maxActiveMatches,
        (
          SELECT COUNT(*)
          FROM match_participants mp
          JOIN matches m ON m.id = mp.matchId
          WHERE mp.userId = s.userId
            AND m.pool = s.pool
            AND m.phase IN ('pending', 'starting', 'active')
        ) AS activeMatches,
        r.rating,
        r.deviation
      FROM seeks s
      LEFT JOIN ratings r ON r.userId = s.userId AND r.pool = s.pool
      WHERE s.pool = ?
      ORDER BY s.createdAt, s.userId
      LIMIT ?
    `)
    .bind(pool, CANDIDATE_LIMIT)
    .all<CandidateRow>();

  const candidates: MatchmakingCandidate[] = candidateResult.results.map((row) => ({
    userId: row.userId,
    pool: row.pool,
    generation: row.generation,
    createdAt: new Date(row.createdAt * 1000),
    maxActiveMatches: row.maxActiveMatches,
    activeMatches: Number(row.activeMatches),
    rating: row.rating ?? INITIAL_RATING,
    deviation: row.deviation ?? INITIAL_DEVIATION,
  }));
  if (candidates.length < 2) return 0;

  const activeResult = await database
    .prepare(`
      SELECT p.userOneId, p.userTwoId
      FROM pairings p
      JOIN matches m ON m.id = p.matchId
      WHERE p.pool = ? AND m.phase IN ('pending', 'starting', 'active')
    `)
    .bind(pool)
    .all<ActivePairRow>();
  const activePairs = new Set(
    activeResult.results.map((row) => userPairKey(row.userOneId, row.userTwoId)),
  );
  const selected = selectMatchmakingPairs(candidates, now, activePairs);
  if (selected.length === 0) return 0;

  const mapResult = await database
    .prepare(`
      SELECT rm.mapId, m.currentRevision AS revision
      FROM ranked_maps rm
      JOIN maps m ON m.id = rm.mapId
      JOIN map_revisions mr ON mr.mapId = m.id AND mr.revision = m.currentRevision
      WHERE rm.pool = ? AND rm.season = ? AND mr.playerCount = 2
      ORDER BY rm.mapId
      LIMIT 100
    `)
    .bind(pool, season)
    .all<RankedMapRow>();
  if (mapResult.results.length === 0) return 0;

  let created = 0;
  for (const selectedPair of selected) {
    const map = mapResult.results[randomIndex(mapResult.results.length)]!;
    const mapDocument = await loadMapRevision({ mapId: map.mapId, revision: map.revision });
    const factions = mapSlotFactionIds(mapDocument);
    const pair =
      crypto.getRandomValues(new Uint8Array(1))[0]! % 2 === 0
        ? [selectedPair.first, selectedPair.second]
        : [selectedPair.second, selectedPair.first];
    const matchId = generateMatchId();
    const pairingId = nanoid();
    const deadlineAt = new Date(now.getTime() + CONFIRMATION_WINDOW_MS);
    const canonical = [selectedPair.first, selectedPair.second].sort((a, b) =>
      a.userId.localeCompare(b.userId),
    );
    const settings = JSON.stringify({
      fogEnabled: pool === "fog_async" || pool === "fog_live",
      startingFunds: 1000,
      hotseatEnabled: false,
      bannedCoIds: [],
      // Settings without a clock do not parse, so a pairing made without one
      // cannot be read back. The ranked pools take the same clock a host gets
      // by default until the ranked clock values are decided.
      clock: defaultMatchClock,
    });

    const results = await database.batch([
      database
        .prepare(`
          INSERT INTO matches (
            id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers,
            isPrivate, joinSlug, settings, createdAt, updatedAt, pool, season
          )
          SELECT ?, ?, 'pending', ?, ?, ?, 2, 1, NULL, ?, ?, ?, ?, ?
          WHERE EXISTS (
            SELECT 1 FROM seeks WHERE userId = ? AND pool = ? AND generation = ?
          ) AND EXISTS (
            SELECT 1 FROM seeks WHERE userId = ? AND pool = ? AND generation = ?
          ) AND NOT EXISTS (
            SELECT 1
            FROM pairings p
            JOIN matches active ON active.id = p.matchId
            WHERE p.pool = ? AND p.userOneId = ? AND p.userTwoId = ?
              AND active.phase IN ('pending', 'starting', 'active')
          )
        `)
        .bind(
          matchId,
          `Ranked ${pool}`,
          pair[0]!.userId,
          map.mapId,
          map.revision,
          settings,
          Math.floor(now.getTime() / 1000),
          Math.floor(now.getTime() / 1000),
          pool,
          season,
          selectedPair.first.userId,
          pool,
          selectedPair.first.generation,
          selectedPair.second.userId,
          pool,
          selectedPair.second.generation,
          pool,
          canonical[0]!.userId,
          canonical[1]!.userId,
        ),
      ...pair.map((candidate, slotIndex) =>
        database
          .prepare(`
            INSERT INTO match_participants (
              matchId, userId, slotIndex, factionId, coId, ready, joinedAt, updatedAt
            )
            SELECT ?, ?, ?, ?, NULL, 0, ?, ? FROM matches WHERE id = ?
          `)
          .bind(
            matchId,
            candidate.userId,
            slotIndex,
            factions[slotIndex]!,
            Math.floor(now.getTime() / 1000),
            Math.floor(now.getTime() / 1000),
            matchId,
          ),
      ),
      database
        .prepare(`
          INSERT INTO pairings (
            id, matchId, pool, season, userOneId, userTwoId,
            userOneSeekGeneration, userTwoSeekGeneration, status, createdAt, deadlineAt
          )
          SELECT ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ? FROM matches WHERE id = ?
        `)
        .bind(
          pairingId,
          matchId,
          pool,
          season,
          canonical[0]!.userId,
          canonical[1]!.userId,
          canonical[0]!.generation,
          canonical[1]!.generation,
          Math.floor(now.getTime() / 1000),
          Math.floor(deadlineAt.getTime() / 1000),
          matchId,
        ),
    ]);
    if ((results[0]?.meta.changes ?? 0) === 1) {
      created += 1;
      activePairs.add(userPairKey(selectedPair.first.userId, selectedPair.second.userId));
    }
  }
  return created;
}

/** Expire every confirmation window which is still pending. */
export async function expirePendingPairings(
  database: D1Database,
  now = new Date(),
): Promise<number> {
  const seconds = Math.floor(now.getTime() / 1000);
  const results = await database.batch([
    // A player who let the window close is a player who has stopped looking.
    // Their seek stops with it, because the next pairing would take another
    // opponent out of the pool for 24 hours to reach the same end. The player
    // starts the seek again themselves. A player who declined made a choice
    // and keeps their seek; only silence pauses it.
    //
    // This runs before the pairing rows leave the pending status, which is
    // what makes the select above find them.
    database
      .prepare(`
      DELETE FROM seeks
      WHERE EXISTS (
        SELECT 1
        FROM pairings p
        JOIN match_participants mp ON mp.matchId = p.matchId
        WHERE p.status = 'pending'
          AND p.deadlineAt <= ?
          AND mp.ready = 0
          AND mp.userId = seeks.userId
          AND p.pool = seeks.pool
          -- A seek started again after this pairing is a new seek. The
          -- generation tells the two apart, so an expired window only stops
          -- the seek which it came from.
          AND seeks.generation = CASE
            WHEN seeks.userId = p.userOneId THEN p.userOneSeekGeneration
            ELSE p.userTwoSeekGeneration
          END
      )
    `)
      .bind(seconds),
    database
      .prepare(`
      INSERT INTO match_voids (matchId, publicReason, voidedAt)
      SELECT matchId, 'Ranked confirmation expired', ?
      FROM pairings
      WHERE status = 'pending' AND deadlineAt <= ?
      ON CONFLICT(matchId) DO NOTHING
    `)
      .bind(seconds, seconds),
    database
      .prepare(`
      UPDATE matches
      SET phase = 'cancelled', completedAt = ?, updatedAt = ?
      WHERE id IN (
        SELECT matchId FROM pairings WHERE status = 'pending' AND deadlineAt <= ?
      ) AND phase = 'pending'
    `)
      .bind(seconds, seconds, seconds),
    database
      .prepare(`
      UPDATE pairings
      SET status = 'expired', resolvedAt = ?
      WHERE status = 'pending' AND deadlineAt <= ?
    `)
      .bind(seconds, seconds),
  ]);
  return results[3]?.meta.changes ?? 0;
}

export async function nextPairingDeadline(
  database: D1Database,
  pool: RankedPool,
  season: number,
): Promise<number | null> {
  const row = await database
    .prepare(
      "SELECT MIN(deadlineAt) AS deadlineAt FROM pairings WHERE pool = ? AND season = ? AND status = 'pending'",
    )
    .bind(pool, season)
    .first<{ deadlineAt: RawCol<PairingColumns, "deadlineAt"> | null }>();
  return row?.deadlineAt == null ? null : row.deadlineAt * 1000;
}

/** The next complete-hour boundary that widens a seek which is still limited. */
export async function nextSeekWidening(
  database: D1Database,
  pool: RankedPool,
  now = new Date(),
): Promise<number | null> {
  const nowSeconds = Math.floor(now.getTime() / 1000);
  const result = await database
    .prepare(`
      SELECT createdAt
      FROM seeks
      WHERE pool = ? AND createdAt > ? AND createdAt <= ?
      ORDER BY createdAt
      LIMIT ?
    `)
    .bind(pool, nowSeconds - 24 * 60 * 60, nowSeconds, CANDIDATE_LIMIT)
    .all<RawRow<SeekColumns, "createdAt">>();
  let next: number | null = null;
  for (const row of result.results) {
    const ageHours = Math.floor((nowSeconds - row.createdAt) / (60 * 60));
    const boundary = (row.createdAt + (ageHours + 1) * 60 * 60) * 1000;
    if (next === null || boundary < next) next = boundary;
  }
  return next;
}

function randomIndex(length: number): number {
  const range = Math.floor(0x1_0000_0000 / length) * length;
  const values = new Uint32Array(1);
  do crypto.getRandomValues(values);
  while (values[0]! >= range);
  return values[0]! % length;
}

export function isAllowedRankedCo(coId: number): boolean {
  return isKnownCoId(coId);
}

/*
 * Everything the ranked hub reads, in two queries per surface.
 *
 * The reads are deliberately narrow. The hub is allowed to describe the
 * viewer: their rating, their seek, their games, their place in the
 * standings. It is not allowed to describe the pool, so nothing here counts
 * seeks, opponents waiting, or players in a season.
 */

import { and, desc, eq, exists, inArray, isNotNull, ne, sql } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { alias } from "drizzle-orm/sqlite-core";
import { env } from "cloudflare:workers";
import {
  maps,
  matchParticipants,
  matches,
  pairings,
  ratings,
  seasons,
  seeks,
  user,
} from "#/db/global.ts";
import type { MatchPhase, RankedPool } from "#/matches/schemas.ts";
import { rankedPools } from "#/matches/schemas.ts";
import {
  RANKED_POOL_ORDER,
  isProvisional,
  isRankedPoolOpen,
  readTimeDeviation,
} from "./ranked_display.ts";
import { DEFAULT_MAX_ACTIVE_MATCHES, INITIAL_RATING } from "./matchmaking.ts";
import { readLadder } from "./ratings.server.ts";
import { activeSeasonNumber } from "./matchmaking.server.ts";

/** The number of rows the standings panel reads. */
const STANDINGS_LIMIT = 100;

/** The phases which hold a ranked slot: the match is not over. */
const ONGOING_RANKED_PHASES: readonly MatchPhase[] = ["pending", "starting", "active"];

export interface RankedRatingSnapshot {
  rating: number;
  /** The deviation after the growth for time without a rated match. */
  deviation: number;
  ratedMatches: number;
  isProvisional: boolean;
}

export interface RankedPendingSummary {
  matchId: string;
  mapName: string;
  mapId: string;
  mapRevision: number;
  slotIndex: number;
  factionId: number;
  hasCommander: boolean;
  isReady: boolean;
  /** When the confirmation window closes. */
  deadlineAt: string;
}

export interface RankedInPlaySummary {
  matchId: string;
  mapName: string;
  mapId: string;
  mapRevision: number;
  slotIndex: number;
  factionId: number;
  coId: number | null;
  /** Null while the match is pending, because the opponent is not revealed. */
  opponentName: string | null;
  updatedAt: string;
}

export interface RankedPoolSnapshot {
  pool: RankedPool;
  isOpen: boolean;
  seek: { maxActiveMatches: number; createdAt: string } | null;
  rating: RankedRatingSnapshot | null;
  /** Pending, starting, and active ranked matches in this pool. */
  activeMatches: number;
  pending: RankedPendingSummary[];
  inPlay: RankedInPlaySummary[];
}

export interface RankedOverview {
  isEmailVerified: boolean;
  season: { number: number; startsAt: string; endsAt: string } | null;
  pools: RankedPoolSnapshot[];
  /** The default the capacity control starts on for a pool with no seek. */
  defaultMaxActiveMatches: number;
  loadedAt: string;
}

export async function rankedOverview(userId: string, now = new Date()): Promise<RankedOverview> {
  const database = drizzle(env.DB, {
    schema: { maps, matchParticipants, matches, pairings, ratings, seasons, seeks, user },
  });

  // The opponent reaches the row through the seat which is not the viewer's.
  const seat = alias(matchParticipants, "seat");
  const opponent = alias(user, "opponent");

  const [seekRows, ratingRows, matchRows] = await database.batch([
    database
      .select({
        pool: seeks.pool,
        maxActiveMatches: seeks.maxActiveMatches,
        createdAt: seeks.createdAt,
      })
      .from(seeks)
      .where(eq(seeks.userId, userId)),
    database
      .select({
        pool: ratings.pool,
        rating: ratings.rating,
        deviation: ratings.deviation,
        ratedMatches: ratings.ratedMatches,
        lastRatedAt: ratings.lastRatedAt,
      })
      .from(ratings)
      .where(eq(ratings.userId, userId)),
    database
      .select({
        matchId: matches.id,
        pool: matches.pool,
        phase: matches.phase,
        mapId: matches.mapId,
        mapRevision: matches.mapRevision,
        updatedAt: matches.updatedAt,
        slotIndex: matchParticipants.slotIndex,
        factionId: matchParticipants.factionId,
        coId: matchParticipants.coId,
        ready: matchParticipants.ready,
        deadlineAt: pairings.deadlineAt,
        // D1 returns one object per row, keyed by column name, so the two
        // `name` columns need distinct names or the later one wins.
        mapName: sql<string>`${maps.name}`.as("mapName"),
        opponentName: sql<string | null>`${opponent.name}`.as("opponentName"),
      })
      .from(matches)
      .innerJoin(
        matchParticipants,
        and(eq(matchParticipants.matchId, matches.id), eq(matchParticipants.userId, userId)),
      )
      .innerJoin(maps, eq(maps.id, matches.mapId))
      .leftJoin(pairings, eq(pairings.matchId, matches.id))
      .leftJoin(seat, and(eq(seat.matchId, matches.id), ne(seat.userId, userId)))
      .leftJoin(opponent, eq(opponent.id, seat.userId))
      .where(and(isNotNull(matches.pool), inArray(matches.phase, ONGOING_RANKED_PHASES)))
      .orderBy(desc(matches.updatedAt)),
  ]);

  const seasonNumber = await activeSeasonNumber(now);
  const [seasonRow, account] = await Promise.all([
    seasonNumber === null
      ? Promise.resolve(undefined)
      : database.select().from(seasons).where(eq(seasons.number, seasonNumber)).get(),
    database
      .select({ emailVerified: user.emailVerified })
      .from(user)
      .where(eq(user.id, userId))
      .get(),
  ]);

  const pools = RANKED_POOL_ORDER.map((pool): RankedPoolSnapshot => {
    const seekRow = seekRows.find((row) => row.pool === pool);
    const ratingRow = ratingRows.find((row) => row.pool === pool);
    const poolMatches = matchRows.filter((row) => row.pool === pool);
    const hasRatedMatchInProgress = poolMatches.length > 0;

    return {
      pool,
      isOpen: isRankedPoolOpen(pool),
      seek: seekRow
        ? {
            maxActiveMatches: seekRow.maxActiveMatches,
            createdAt: seekRow.createdAt.toISOString(),
          }
        : null,
      rating: ratingRow ? toRatingSnapshot(ratingRow, now, hasRatedMatchInProgress) : null,
      activeMatches: poolMatches.length,
      pending: poolMatches
        .filter((row) => row.phase === "pending")
        // A pending match without a pairing cannot happen, but the outer join
        // allows it, and the match time is the closest stand-in for a deadline.
        .map((row) => ({ row, deadlineAt: row.deadlineAt ?? row.updatedAt }))
        .sort((left, right) => left.deadlineAt.getTime() - right.deadlineAt.getTime())
        .map(({ row, deadlineAt }) => ({
          matchId: row.matchId,
          mapName: row.mapName,
          mapId: row.mapId,
          mapRevision: row.mapRevision,
          slotIndex: row.slotIndex,
          factionId: row.factionId,
          // The commander is a fact about the viewer's own seat, so it stays.
          hasCommander: row.coId !== null,
          isReady: row.ready,
          deadlineAt: deadlineAt.toISOString(),
        })),
      inPlay: poolMatches
        .filter((row) => row.phase !== "pending")
        .map((row) => ({
          matchId: row.matchId,
          mapName: row.mapName,
          mapId: row.mapId,
          mapRevision: row.mapRevision,
          slotIndex: row.slotIndex,
          factionId: row.factionId,
          coId: row.coId,
          opponentName: row.opponentName,
          updatedAt: row.updatedAt.toISOString(),
        })),
    };
  });

  return {
    isEmailVerified: account?.emailVerified === true,
    season: seasonRow
      ? {
          number: seasonRow.number,
          startsAt: seasonRow.startsAt.toISOString(),
          endsAt: seasonRow.endsAt.toISOString(),
        }
      : null,
    pools,
    defaultMaxActiveMatches: DEFAULT_MAX_ACTIVE_MATCHES,
    loadedAt: now.toISOString(),
  };
}

function toRatingSnapshot(
  row: { rating: number; deviation: number; ratedMatches: number; lastRatedAt: Date | null },
  now: Date,
  hasRatedMatchInProgress: boolean,
): RankedRatingSnapshot {
  const deviation = readTimeDeviation(row, now, hasRatedMatchInProgress);
  return {
    rating: row.rating,
    deviation,
    ratedMatches: row.ratedMatches,
    isProvisional: isProvisional(deviation),
  };
}

export interface StandingsEntry {
  rank: number;
  userId: string;
  name: string;
  rating: number;
  ratedMatches: number;
  isViewer: boolean;
}

export interface RankedStandings {
  pool: RankedPool;
  entries: StandingsEntry[];
  /**
   * The viewer's own state when they are not listed. A player enters the
   * standings when a rated match brings the deviation down to the confirmed
   * range.
   */
  viewer: {
    rating: number;
    ratedMatches: number;
    isProvisional: boolean;
  } | null;
}

export async function rankedStandings(
  pool: RankedPool,
  viewerUserId: string | null,
  now = new Date(),
): Promise<RankedStandings> {
  if (!rankedPools.includes(pool)) throw new Error("unknown ranked pool");

  /*
   * The whole pool is read and then cut down, rather than being cut down by
   * the database. The order is `ladderScore`, which needs the deviation that
   * time has grown, and that growth is worked out here and not in SQL. Cutting
   * the rows down first would drop players the order was going to lift.
   */
  const ladder = await readLadder(env.DB, pool, now);
  const entries: StandingsEntry[] = ladder.slice(0, STANDINGS_LIMIT).map((row, index) => ({
    rank: index + 1,
    userId: row.userId,
    name: row.name,
    rating: row.rating,
    ratedMatches: row.ratedMatches,
    isViewer: row.userId === viewerUserId,
  }));

  if (viewerUserId === null || entries.some((entry) => entry.isViewer)) {
    return { pool, entries, viewer: null };
  }

  // The viewer holds a place further down the ladder than the panel lists.
  const placed = ladder.find((row) => row.userId === viewerUserId);
  if (placed) {
    return {
      pool,
      entries,
      viewer: {
        rating: placed.rating,
        ratedMatches: placed.ratedMatches,
        isProvisional: isProvisional(placed.deviation),
      },
    };
  }

  // The viewer holds no place: they are new, or they have been away too long.
  const database = drizzle(env.DB, { schema: { matchParticipants, matches, ratings, user } });
  const own = await database
    .select({
      rating: ratings.rating,
      deviation: ratings.deviation,
      ratedMatches: ratings.ratedMatches,
      lastRatedAt: ratings.lastRatedAt,
      // The deviation stops growing while a rated match is under way.
      inProgress: exists(
        database
          .select({ one: sql`1` })
          .from(matchParticipants)
          .innerJoin(matches, eq(matches.id, matchParticipants.matchId))
          .where(
            and(
              eq(matchParticipants.userId, ratings.userId),
              eq(matches.pool, ratings.pool),
              inArray(matches.phase, ONGOING_RANKED_PHASES),
            ),
          ),
      ).mapWith(Boolean),
    })
    .from(ratings)
    .where(and(eq(ratings.userId, viewerUserId), eq(ratings.pool, pool)))
    .get();

  return {
    pool,
    entries,
    viewer: own
      ? {
          rating: own.rating,
          ratedMatches: own.ratedMatches,
          isProvisional: isProvisional(readTimeDeviation(own, now, own.inProgress)),
        }
      : { rating: INITIAL_RATING, ratedMatches: 0, isProvisional: true },
  };
}

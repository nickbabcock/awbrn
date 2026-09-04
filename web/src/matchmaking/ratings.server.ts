/*
 * The rating pass: what a finished match does to the two ratings behind it.
 *
 * Every write here goes through one durable object for each pool, so a rating
 * row has one writer and no two passes can race for it. That is why nothing in
 * this file compares and swaps. What it does guard against is the same result
 * being read twice, because a durable object which stops halfway wakes and
 * tries again: `match_results.ratedAt` is the queue, the receipt, and the
 * stamp which keeps one match from moving a rating twice. The stamp and the
 * rating are written in one batch, so a pass that fails leaves neither.
 */

import { and, asc, eq, gt, inArray, isNotNull, isNull, sql } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import {
  matchResults,
  matchVoids,
  matches,
  ratingUpdates,
  ratings,
  seasonCaptures,
  seasonStandings,
  seasons,
  user,
} from "#/db/global.ts";
import type { MatchOutcome, MatchPhase, RankedPool } from "#/matches/schemas.ts";
import { glickoScore } from "#/matches/match_results.ts";
import { updateRating, type GlickoState } from "./glicko.ts";
import { isListedOnLadder, ladderScore, readTimeDeviation } from "./ranked_display.ts";
import { INITIAL_DEVIATION, INITIAL_RATING } from "./matchmaking.ts";

/** The volatility a player starts with, which is Glickman's suggestion. */
export const INITIAL_VOLATILITY = 0.06;

/**
 * How many matches one pass reads. A pass which fills it is run again.
 *
 * The seats are read by the identifiers the first query returns, so the batch
 * stays well under the number of values D1 binds into one statement.
 */
const RESULT_BATCH_LIMIT = 50;

/** How many identifiers go into one statement, for the same reason. */
const STAMP_CHUNK = 40;

/** The phases which mean a match has not finished. */
const ONGOING_PHASES: readonly MatchPhase[] = ["pending", "starting", "active"];

/** One rating move, as the player is told about it. */
export interface AppliedRating {
  matchId: string;
  userId: string;
  pool: RankedPool;
  ratingBefore: number;
  ratingAfter: number;
}

interface PendingSeat {
  matchId: string;
  userId: string;
  outcome: MatchOutcome;
  season: number;
  recordedAt: Date;
}

/** A match with both of its seats, ready to rate. */
interface PendingMatch {
  matchId: string;
  season: number;
  recordedAt: Date;
  seats: [PendingSeat, PendingSeat];
}

function initialState(): GlickoState {
  return {
    rating: INITIAL_RATING,
    deviation: INITIAL_DEVIATION,
    volatility: INITIAL_VOLATILITY,
  };
}

/**
 * Group the seats of one pool's unrated results into matches.
 *
 * A match which does not have exactly two seats held by two different people
 * cannot be rated. It is returned as unratable so that the pass stamps it and
 * stops reading it on every wake.
 */
export function groupPendingSeats(seats: readonly PendingSeat[]): {
  matches: PendingMatch[];
  unratable: string[];
} {
  const bySeat = new Map<string, PendingSeat[]>();
  for (const seat of seats) {
    const group = bySeat.get(seat.matchId);
    if (group) group.push(seat);
    else bySeat.set(seat.matchId, [seat]);
  }

  const ready: PendingMatch[] = [];
  const unratable: string[] = [];
  for (const [matchId, group] of bySeat) {
    const [first, second] = group;
    if (group.length !== 2 || !first || !second || first.userId === second.userId) {
      unratable.push(matchId);
      continue;
    }
    ready.push({
      matchId,
      season: first.season,
      recordedAt: first.recordedAt,
      seats: [first, second],
    });
  }

  // Oldest first, so a player in two matches is rated in the order they played.
  ready.sort((left, right) => left.recordedAt.getTime() - right.recordedAt.getTime());
  return { matches: ready, unratable };
}

/**
 * Rate every finished match in one pool which has not been rated yet.
 *
 * Returns what changed, so the caller can tell the players, and whether the
 * queue is empty. A pass which filled either of its batches leaves `drained`
 * false, because a full batch is the sign that there is more behind it.
 */
export async function applyPendingRatings(
  database: D1Database,
  pool: RankedPool,
  now = new Date(),
): Promise<{ applied: AppliedRating[]; drained: boolean }> {
  const db = drizzle(database);

  const readable = and(
    eq(matchResults.pool, pool),
    isNull(matchResults.ratedAt),
    isNotNull(matchResults.userId),
    isNotNull(matches.season),
  );

  // The batch counts matches and not rows. A limit on the rows would cut a
  // match between its two seats, and a match read with one seat is stamped as
  // unratable, so the queue would eat the very results it is here to rate.
  const queued = await db
    .select({ matchId: matchResults.matchId })
    .from(matchResults)
    .innerJoin(matches, eq(matches.id, matchResults.matchId))
    .leftJoin(matchVoids, eq(matchVoids.matchId, matchResults.matchId))
    .where(and(readable, isNull(matchVoids.matchId)))
    .groupBy(matchResults.matchId)
    .orderBy(sql`min(${matchResults.recordedAt}) asc`, asc(matchResults.matchId))
    .limit(RESULT_BATCH_LIMIT);

  const queuedIds = queued.map((row) => row.matchId);
  const pending =
    queuedIds.length === 0
      ? []
      : // No limit here: every seat of the matches picked above is read.
        await db
          .select({
            matchId: matchResults.matchId,
            userId: matchResults.userId,
            outcome: matchResults.outcome,
            recordedAt: matchResults.recordedAt,
            season: matches.season,
          })
          .from(matchResults)
          .innerJoin(matches, eq(matches.id, matchResults.matchId))
          .where(and(readable, inArray(matchResults.matchId, queuedIds)))
          .orderBy(asc(matchResults.recordedAt), asc(matchResults.matchId));

  // A voided result is read once and stamped, so the queue does not keep it.
  const voided = await db
    .select({ matchId: matchResults.matchId })
    .from(matchResults)
    .innerJoin(matchVoids, eq(matchVoids.matchId, matchResults.matchId))
    .where(and(eq(matchResults.pool, pool), isNull(matchResults.ratedAt)))
    .limit(RESULT_BATCH_LIMIT);

  const seats: PendingSeat[] = pending.flatMap((row) =>
    row.userId === null || row.season === null
      ? []
      : [
          {
            matchId: row.matchId,
            userId: row.userId,
            outcome: row.outcome,
            season: row.season,
            recordedAt: row.recordedAt,
          },
        ],
  );

  // A full batch of either kind means the pass stopped short of the queue's end.
  const drained = queued.length < RESULT_BATCH_LIMIT && voided.length < RESULT_BATCH_LIMIT;

  const { matches: ready, unratable } = groupPendingSeats(seats);
  const skipped = [...new Set([...unratable, ...voided.map((row) => row.matchId)])];
  for (let start = 0; start < skipped.length; start += STAMP_CHUNK) {
    await db
      .update(matchResults)
      .set({ ratedAt: now })
      .where(
        and(
          inArray(matchResults.matchId, skipped.slice(start, start + STAMP_CHUNK)),
          isNull(matchResults.ratedAt),
        ),
      );
  }

  if (ready.length === 0) return { applied: [], drained };

  // One read for every rating the pass touches, then the pass works from the
  // states it holds. A player in two of these matches is rated twice, and the
  // second match reads what the first one left.
  const userIds = [...new Set(ready.flatMap((match) => match.seats.map((seat) => seat.userId)))];
  const stored = await db
    .select({
      userId: ratings.userId,
      rating: ratings.rating,
      deviation: ratings.deviation,
      volatility: ratings.volatility,
      lastRatedAt: ratings.lastRatedAt,
      ratedMatches: ratings.ratedMatches,
    })
    .from(ratings)
    .where(and(eq(ratings.pool, pool), inArray(ratings.userId, userIds)));

  const current = new Map(stored.map((row) => [row.userId, row]));
  const applied: AppliedRating[] = [];

  for (const match of ready) {
    const states = match.seats.map((seat) => {
      const row = current.get(seat.userId);
      const base = row
        ? { rating: row.rating, deviation: row.deviation, volatility: row.volatility }
        : initialState();
      return {
        seat,
        ratedMatches: row?.ratedMatches ?? 0,
        // Glicko-2 widens a rating which has not been tested lately. The
        // growth is measured to the match and not to now, because a result
        // which waited in the queue did not make the rating any older.
        before: {
          ...base,
          deviation: readTimeDeviation(
            { deviation: base.deviation, lastRatedAt: row?.lastRatedAt ?? null },
            match.recordedAt,
            false,
          ),
        },
      };
    });

    const [firstState, secondState] = states;
    if (!firstState || !secondState) continue;

    // Both players are rated against what the other brought to the match, so
    // the order the two are written in cannot change either result.
    const results = [
      { self: firstState, opponent: secondState },
      { self: secondState, opponent: firstState },
    ].map(({ self, opponent }) => {
      const score = glickoScore(self.seat.outcome);
      return {
        seat: self.seat,
        ratedMatches: self.ratedMatches,
        before: self.before,
        opponent: opponent.before,
        score,
        after: updateRating(self.before, [
          {
            rating: opponent.before.rating,
            deviation: opponent.before.deviation,
            score,
          },
        ]),
      };
    });

    await db.batch([
      db
        .update(matchResults)
        .set({ ratedAt: now })
        .where(and(eq(matchResults.matchId, match.matchId), isNull(matchResults.ratedAt))),
      ...results.map((result) =>
        db
          .insert(ratings)
          .values({
            userId: result.seat.userId,
            pool,
            rating: result.after.rating,
            deviation: result.after.deviation,
            volatility: result.after.volatility,
            lastRatedAt: match.recordedAt,
            ratedMatches: result.ratedMatches + 1,
          })
          .onConflictDoUpdate({
            target: [ratings.userId, ratings.pool],
            set: {
              rating: result.after.rating,
              deviation: result.after.deviation,
              volatility: result.after.volatility,
              lastRatedAt: match.recordedAt,
              ratedMatches: sql`${ratings.ratedMatches} + 1`,
            },
          }),
      ),
      ...results.map((result) =>
        db
          .insert(ratingUpdates)
          .values({
            matchId: match.matchId,
            userId: result.seat.userId,
            pool,
            season: match.season,
            ratingBefore: result.before.rating,
            ratingAfter: result.after.rating,
            deviationBefore: result.before.deviation,
            deviationAfter: result.after.deviation,
            volatilityBefore: result.before.volatility,
            volatilityAfter: result.after.volatility,
            opponentRating: result.opponent.rating,
            opponentDeviation: result.opponent.deviation,
            score: result.score,
            appliedAt: now,
          })
          .onConflictDoNothing(),
      ),
    ]);

    for (const result of results) {
      current.set(result.seat.userId, {
        userId: result.seat.userId,
        rating: result.after.rating,
        deviation: result.after.deviation,
        volatility: result.after.volatility,
        lastRatedAt: match.recordedAt,
        ratedMatches: result.ratedMatches + 1,
      });
      applied.push({
        matchId: match.matchId,
        userId: result.seat.userId,
        pool,
        ratingBefore: result.before.rating,
        ratingAfter: result.after.rating,
      });
    }
  }

  return { applied, drained };
}

/**
 * Freeze the ladder for every season of this pool which has closed.
 *
 * A season closes when its end date has passed, no match of that season is
 * still being played, and every result of that season has been rated. Async
 * matches run over days and cross the boundary, so the calendar alone is not
 * enough to know the season is over.
 */
export async function captureClosedSeasons(
  database: D1Database,
  pool: RankedPool,
  now = new Date(),
): Promise<number[]> {
  const db = drizzle(database);

  const closed = await db
    .select({ number: seasons.number })
    .from(seasons)
    .where(
      and(
        sql`${seasons.endsAt} <= ${Math.floor(now.getTime() / 1000)}`,
        sql`not exists (select 1 from ${seasonCaptures}
              where ${seasonCaptures.season} = ${seasons.number}
                and ${seasonCaptures.pool} = ${pool})`,
        sql`not exists (select 1 from ${matches}
              where ${matches.season} = ${seasons.number}
                and ${matches.pool} = ${pool}
                and ${matches.phase} in (${sql.join(
                  ONGOING_PHASES.map((phase) => sql`${phase}`),
                  sql`, `,
                )}))`,
        sql`not exists (select 1 from ${matchResults}
              inner join ${matches} on ${matches.id} = ${matchResults.matchId}
              where ${matches.season} = ${seasons.number}
                and ${matchResults.pool} = ${pool}
                and ${matchResults.ratedAt} is null)`,
      ),
    )
    .orderBy(asc(seasons.number));

  const captured: number[] = [];
  for (const season of closed) {
    // Not the live ladder: the last match of a season can be rated long after
    // the next season started, and by then the live rows hold moves which
    // belong to the newer season. The freeze reads the older season's own end.
    const rows = await readSeasonLadder(database, pool, season.number, now);
    const seasonCounts = await db
      .select({
        userId: ratingUpdates.userId,
        played: sql<number>`count(*)`.as("played"),
      })
      .from(ratingUpdates)
      .where(and(eq(ratingUpdates.pool, pool), eq(ratingUpdates.season, season.number)))
      .groupBy(ratingUpdates.userId);
    const played = new Map(seasonCounts.map((row) => [row.userId, Number(row.played)]));

    // The freeze is one write. The row which records it goes in with the
    // places, so a season is never half frozen and never frozen twice.
    const placeRows = rows.map((row, index) => ({
      season: season.number,
      pool,
      rank: index + 1,
      userId: row.userId,
      name: row.name,
      rating: row.rating,
      // The grown value, so the frozen row never has to be grown again.
      deviation: row.deviation,
      ratedMatches: row.ratedMatches,
      seasonMatches: played.get(row.userId) ?? 0,
      capturedAt: now,
    }));

    const capture = db
      .insert(seasonCaptures)
      .values({ season: season.number, pool, placeCount: placeRows.length, capturedAt: now })
      .onConflictDoNothing();

    if (placeRows.length > 0) {
      await db.batch([capture, db.insert(seasonStandings).values(placeRows).onConflictDoNothing()]);
    } else {
      await capture;
    }
    captured.push(season.number);
  }

  return captured;
}

/** One place on the ladder, after the deviation has been grown to now. */
export interface LadderRow {
  userId: string;
  name: string;
  rating: number;
  /** Grown to the moment it was read. */
  deviation: number;
  ratedMatches: number;
  score: number;
}

/**
 * The live ladder for one pool, in order.
 *
 * The order is `ladderScore`, so a rating nobody has tested lately slides down
 * instead of holding its place. The deviation is grown here and not in SQL,
 * which is why the whole pool is read and then cut down rather than being cut
 * down by the database.
 */
export async function readLadder(
  database: D1Database,
  pool: RankedPool,
  now = new Date(),
  limit?: number,
): Promise<LadderRow[]> {
  const rows = await readRatingRows(drizzle(database), pool);
  return listLadder(rows, now, limit);
}

/**
 * The ladder as one closed season left it.
 *
 * A rating carries into the next season, so by the time the last match of a
 * season is rated the live rows can already hold moves which belong to the
 * season after it. Those moves are taken back here. `rating_updates` keeps
 * what every pass read, so the first update of a later season holds the
 * rating and the deviation the player ended the closed season with.
 */
export async function readSeasonLadder(
  database: D1Database,
  pool: RankedPool,
  season: number,
  now = new Date(),
): Promise<LadderRow[]> {
  const db = drizzle(database);
  const rows = await readRatingRows(db, pool);

  const later = await db
    .select({
      userId: ratingUpdates.userId,
      matchId: ratingUpdates.matchId,
      ratingBefore: ratingUpdates.ratingBefore,
      deviationBefore: ratingUpdates.deviationBefore,
      recordedAt: matchResults.recordedAt,
    })
    .from(ratingUpdates)
    .innerJoin(
      matchResults,
      and(
        eq(matchResults.matchId, ratingUpdates.matchId),
        eq(matchResults.userId, ratingUpdates.userId),
      ),
    )
    .where(and(eq(ratingUpdates.pool, pool), gt(ratingUpdates.season, season)))
    .orderBy(asc(matchResults.recordedAt), asc(ratingUpdates.matchId));

  const undone = new Map<string, { rating: number; deviation: number; at: Date; moves: number }>();
  for (const row of later) {
    const held = undone.get(row.userId);
    if (held) {
      held.moves += 1;
      continue;
    }
    undone.set(row.userId, {
      rating: row.ratingBefore,
      // The deviation the pass read, which is already grown to that match. It
      // is paired with that match's own time below, so the growth to now is
      // measured from there and is never counted twice.
      deviation: row.deviationBefore,
      at: row.recordedAt,
      moves: 1,
    });
  }

  const restored = rows.flatMap((row) => {
    const back = undone.get(row.userId);
    if (!back) return [row];
    const ratedMatches = row.ratedMatches - back.moves;
    if (ratedMatches <= 0) return [];
    return [
      {
        ...row,
        rating: back.rating,
        deviation: back.deviation,
        lastRatedAt: back.at,
        ratedMatches,
      },
    ];
  });

  return listLadder(restored, now);
}

/** One rating row, before time has been counted against its deviation. */
interface RatingSnapshot {
  userId: string;
  name: string;
  rating: number;
  deviation: number;
  ratedMatches: number;
  lastRatedAt: Date | null;
  inProgress: number;
}

function readRatingRows(
  db: ReturnType<typeof drizzle>,
  pool: RankedPool,
): Promise<RatingSnapshot[]> {
  return db
    .select({
      userId: ratings.userId,
      name: user.name,
      rating: ratings.rating,
      deviation: ratings.deviation,
      ratedMatches: ratings.ratedMatches,
      lastRatedAt: ratings.lastRatedAt,
      inProgress: sql<number>`exists (
        select 1 from ${matches}
        inner join match_participants mp on mp.matchId = ${matches.id}
        where mp.userId = ${ratings.userId}
          and ${matches.pool} = ${ratings.pool}
          and ${matches.phase} in (${sql.join(
            ONGOING_PHASES.map((phase) => sql`${phase}`),
            sql`, `,
          )})
      )`.as("inProgress"),
    })
    .from(ratings)
    .innerJoin(user, eq(user.id, ratings.userId))
    .where(and(eq(ratings.pool, pool), sql`${ratings.ratedMatches} > 0`));
}

function listLadder(rows: readonly RatingSnapshot[], now: Date, limit?: number): LadderRow[] {
  const listed = rows.flatMap((row) => {
    const deviation = readTimeDeviation(row, now, Boolean(row.inProgress));
    if (!isListedOnLadder(deviation)) return [];
    return [
      {
        userId: row.userId,
        name: row.name,
        rating: row.rating,
        deviation,
        ratedMatches: row.ratedMatches,
        score: ladderScore(row.rating, deviation),
      },
    ];
  });

  listed.sort((left, right) => right.score - left.score || left.name.localeCompare(right.name));
  return limit === undefined ? listed : listed.slice(0, limit);
}

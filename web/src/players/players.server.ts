/*
 * The profile read: everything a visitor may know about one player.
 *
 * Every read is keyed by the player, so each is an index range over their own
 * rows. The one exception is the live ladder place, which reads the rows of
 * the pool that may stand ahead of the player (`readLadderPlace`): the order
 * needs the deviation that time has grown, which is worked out in code.
 */

import { env } from "cloudflare:workers";
import {
  and,
  asc,
  count,
  desc,
  eq,
  inArray,
  isNotNull,
  isNull,
  lte,
  ne,
  notExists,
} from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { alias } from "drizzle-orm/sqlite-core";
import {
  maps,
  matchParticipants,
  matchResults,
  matchVoids,
  matches,
  pairings,
  ratingUpdates,
  ratings,
  seasonStandings,
  seasons,
  user,
} from "#/db/global.ts";
import {
  RANKED_POOL_ORDER,
  isProvisional,
  readTimeDeviation,
} from "#/matchmaking/ranked_display.ts";
import { ONGOING_PHASES, readLadderPlace } from "#/matchmaking/ratings.server.ts";
import { aiSeatName } from "#/matches/ai_profiles.ts";
import { HIDDEN_OPPONENT_NAME } from "#/matches/match_visibility.ts";
import { listedToVisitors } from "#/matches/matches.server.ts";
import { matchSettingsSchema } from "#/matches/schemas.ts";
import { seasonPlaces, thinPoints, topCos } from "./profile_read.ts";
import type { PlayerProfile, ProfileOngoingMatch, ProfilePool, ProfileSeason } from "./schemas.ts";

const db = drizzle(env.DB, {
  schema: {
    maps,
    matchParticipants,
    matchResults,
    matchVoids,
    matches,
    pairings,
    ratingUpdates,
    ratings,
    seasonStandings,
    seasons,
    user,
  },
});

const otherSeats = alias(matchParticipants, "other_seats");

/** A player never has more games in progress than this; the cap only bounds the read. */
const ONGOING_LIMIT = 50;

/** The profile of one player, or null when no account has that id. */
export async function readPlayerProfile(
  userId: string,
  viewerUserId: string | null,
  now = new Date(),
): Promise<PlayerProfile | null> {
  const account = await db
    .select({ id: user.id, name: user.name, createdAt: user.createdAt })
    .from(user)
    .where(eq(user.id, userId))
    .get();
  if (!account) return null;

  const isOwnProfile = viewerUserId === userId;

  const [seasonRows, ratingRows, updateRows, coRows, ongoingRows, poolRows] = await Promise.all([
    db.select().from(seasons).where(lte(seasons.startsAt, now)).orderBy(asc(seasons.number)).all(),
    db
      .select({
        pool: ratings.pool,
        rating: ratings.rating,
        deviation: ratings.deviation,
        lastRatedAt: ratings.lastRatedAt,
        ratedMatches: ratings.ratedMatches,
      })
      .from(ratings)
      .where(eq(ratings.userId, userId))
      .all(),
    db
      .select({
        pool: ratingUpdates.pool,
        season: ratingUpdates.season,
        ratingBefore: ratingUpdates.ratingBefore,
        ratingAfter: ratingUpdates.ratingAfter,
        // When the match was played: one rating pass stamps every update
        // it applies with the same time, and may run after a season closes.
        at: matchResults.recordedAt,
      })
      .from(ratingUpdates)
      .innerJoin(
        matchResults,
        and(
          eq(matchResults.matchId, ratingUpdates.matchId),
          eq(matchResults.userId, ratingUpdates.userId),
        ),
      )
      .where(eq(ratingUpdates.userId, userId))
      .orderBy(asc(ratingUpdates.pool), asc(matchResults.recordedAt), asc(ratingUpdates.matchId))
      .all(),
    db
      .select({
        coId: matchParticipants.coId,
        factionId: matchParticipants.factionId,
        outcome: matchResults.outcome,
        seats: count(),
      })
      .from(matchResults)
      .innerJoin(
        matchParticipants,
        and(
          eq(matchParticipants.matchId, matchResults.matchId),
          eq(matchParticipants.slotIndex, matchResults.slotIndex),
        ),
      )
      .innerJoin(matches, eq(matches.id, matchResults.matchId))
      .leftJoin(matchVoids, eq(matchVoids.matchId, matchResults.matchId))
      .where(
        and(
          eq(matchResults.userId, userId),
          isNotNull(matchParticipants.coId),
          isNull(matchVoids.matchId),
          // A hotseat game puts the player on every side: no record of a CO.
          notExists(
            db
              .select({ slotIndex: otherSeats.slotIndex })
              .from(otherSeats)
              .where(
                and(
                  eq(otherSeats.matchId, matchResults.matchId),
                  eq(otherSeats.userId, userId),
                  ne(otherSeats.slotIndex, matchResults.slotIndex),
                ),
              ),
          ),
          isOwnProfile ? undefined : listedToVisitors(),
        ),
      )
      .groupBy(matchParticipants.coId, matchParticipants.factionId, matchResults.outcome)
      .all(),
    queryOngoingMatches(userId, isOwnProfile),
    // Every pool with a rated match in progress, listed or not: it holds the
    // deviation still, as on the ladder.
    db
      .selectDistinct({ pool: matches.pool })
      .from(matchParticipants)
      .innerJoin(matches, eq(matches.id, matchParticipants.matchId))
      .where(
        and(
          eq(matchParticipants.userId, userId),
          inArray(matches.phase, ONGOING_PHASES),
          isNotNull(matches.pool),
        ),
      )
      .all(),
  ]);

  const currentSeason = seasonRows.find((row) => row.endsAt > now)?.number ?? null;

  const poolsInPlay = new Set(poolRows.map((row) => row.pool));

  const rated = RANKED_POOL_ORDER.flatMap((pool) => {
    const row = ratingRows.find((entry) => entry.pool === pool);
    const updates = updateRows.filter((entry) => entry.pool === pool);
    if (!row || row.ratedMatches === 0 || updates.length === 0) return [];
    return [{ pool, row, updates, deviation: readTimeDeviation(row, now, poolsInPlay.has(pool)) }];
  });

  const seasonNumbers = seasonRows.map((row) => row.number);
  const [places, standingRows] = await Promise.all([
    Promise.all(
      rated.map(({ deviation, pool, row }) =>
        readLadderPlace(env.DB, pool, { userId, rating: row.rating, deviation }, now),
      ),
    ),
    rated.length === 0 || seasonNumbers.length === 0
      ? Promise.resolve([])
      : db
          .select({
            season: seasonStandings.season,
            pool: seasonStandings.pool,
            rank: seasonStandings.rank,
            rating: seasonStandings.rating,
            deviation: seasonStandings.deviation,
            seasonMatches: seasonStandings.seasonMatches,
          })
          .from(seasonStandings)
          // Every key column is bound, so the read is a set of primary key lookups.
          .where(
            and(
              inArray(seasonStandings.season, seasonNumbers),
              inArray(
                seasonStandings.pool,
                rated.map(({ pool }) => pool),
              ),
              eq(seasonStandings.userId, userId),
            ),
          )
          .all(),
  ]);

  const lastSeason = currentSeason ?? seasonNumbers[seasonNumbers.length - 1] ?? null;
  const pools: ProfilePool[] =
    lastSeason === null
      ? []
      : rated.map(({ deviation, pool, row, updates }, index): ProfilePool => {
          const place = places[index]!;
          const games = new Map<number, number>();
          let peak = { at: updates[0]!.at.getTime(), rating: updates[0]!.ratingAfter };
          for (const update of updates) {
            games.set(update.season, (games.get(update.season) ?? 0) + 1);
            if (update.ratingAfter > peak.rating) {
              peak = { at: update.at.getTime(), rating: update.ratingAfter };
            }
          }

          return {
            pool,
            rating: row.rating,
            isProvisional: isProvisional(deviation),
            place,
            start: { at: updates[0]!.at.getTime(), rating: updates[0]!.ratingBefore },
            points: thinPoints(
              updates.map((update): [number, number] => [update.at.getTime(), update.ratingAfter]),
            ),
            peak,
            seasons: seasonPlaces({
              currentSeason,
              firstSeason: updates[0]!.season,
              games,
              lastSeason,
              live: {
                place,
                rating: row.rating,
                deviation,
              },
              standings: new Map(
                standingRows
                  .filter((entry) => entry.pool === pool)
                  .map((entry) => [entry.season, entry]),
              ),
            }),
          };
        });

  const firstSeason = Math.min(...pools.map((entry) => entry.seasons[0]?.season ?? Infinity));
  const profileSeasons: ProfileSeason[] = seasonRows
    .filter((row) =>
      pools.length > 0
        ? row.number >= firstSeason && row.number <= (lastSeason ?? row.number)
        : row.number === lastSeason,
    )
    .map((row) => ({
      number: row.number,
      startsAt: row.startsAt.getTime(),
      endsAt: row.endsAt.getTime(),
    }));

  return {
    userId: account.id,
    name: account.name,
    joinedAt: account.createdAt.getTime(),
    isViewer: isOwnProfile,
    currentSeason,
    seasons: profileSeasons,
    pools,
    cos: topCos(
      coRows.flatMap((row) =>
        row.coId === null
          ? []
          : [
              {
                coId: row.coId,
                factionId: row.factionId,
                outcome: row.outcome,
                seats: Number(row.seats),
              },
            ],
      ),
    ),
    ongoing: await readOngoing(ongoingRows, userId, isOwnProfile),
    loadedAt: now.getTime(),
  };
}

/**
 * The player's games in progress that this visitor may see, one row per match.
 *
 * Someone else never sees a ranked pairing awaiting confirmation (it names
 * the opponent before both players commit) or an invite-only match. The rule
 * is applied before the cap, so hidden games never crowd out listed ones.
 */
function queryOngoingMatches(userId: string, isOwnProfile: boolean) {
  return db
    .select({
      matchId: matches.id,
      name: matches.name,
      phase: matches.phase,
      mapId: matches.mapId,
      mapRevision: matches.mapRevision,
      pool: matches.pool,
      settings: matches.settings,
      activeSlotIndex: matches.activeSlotIndex,
      turnDeadlineAt: matches.turnDeadlineAt,
      pairingDeadlineAt: pairings.deadlineAt,
      mapName: maps.name,
    })
    .from(matches)
    .innerJoin(maps, eq(maps.id, matches.mapId))
    .leftJoin(pairings, eq(pairings.matchId, matches.id))
    .where(
      and(
        inArray(
          matches.id,
          db
            .select({ matchId: matchParticipants.matchId })
            .from(matchParticipants)
            .where(eq(matchParticipants.userId, userId)),
        ),
        inArray(matches.phase, ONGOING_PHASES),
        isOwnProfile ? undefined : and(ne(matches.phase, "pending"), listedToVisitors()),
      ),
    )
    .orderBy(desc(matches.updatedAt), desc(matches.id))
    .limit(ONGOING_LIMIT)
    .all();
}

type OngoingMatchRow = Awaited<ReturnType<typeof queryOngoingMatches>>[number];

/**
 * The games in progress with the seats across from the player. A pairing
 * still awaiting confirmation (listed only on the player's own profile)
 * withholds the opponent, as the pairing does.
 */
async function readOngoing(
  rows: readonly OngoingMatchRow[],
  userId: string,
  isOwnProfile: boolean,
): Promise<ProfileOngoingMatch[]> {
  if (rows.length === 0) return [];

  const seatRows = await db
    .select({
      matchId: matchParticipants.matchId,
      slotIndex: matchParticipants.slotIndex,
      userId: matchParticipants.userId,
      aiProfileId: matchParticipants.aiProfileId,
      factionId: matchParticipants.factionId,
      coId: matchParticipants.coId,
      ready: matchParticipants.ready,
      userName: user.name,
    })
    .from(matchParticipants)
    .leftJoin(user, eq(user.id, matchParticipants.userId))
    .where(
      inArray(
        matchParticipants.matchId,
        rows.map((row) => row.matchId),
      ),
    )
    .orderBy(asc(matchParticipants.slotIndex))
    .all();

  const ongoing = rows.flatMap((row): ProfileOngoingMatch[] => {
    const seats = seatRows.filter((seat) => seat.matchId === row.matchId);
    const held = seats.filter((seat) => seat.userId === userId);
    const settings = matchSettingsSchema.safeParse(row.settings);
    if (!settings.success) return [];
    const ownSlots = new Set(held.map((seat) => seat.slotIndex));
    const isPending = row.phase === "pending";
    const isOwedPairing = isOwnProfile && isPending && held.some((seat) => !seat.ready);
    const isOnMove =
      isOwnProfile &&
      row.phase === "active" &&
      row.activeSlotIndex !== null &&
      ownSlots.has(row.activeSlotIndex);
    const deadline =
      row.phase === "active" ? row.turnDeadlineAt : isPending ? row.pairingDeadlineAt : null;

    return [
      {
        matchId: row.matchId,
        name: row.name,
        mapId: row.mapId,
        mapRevision: row.mapRevision,
        mapName: row.mapName,
        pool: row.pool,
        clock: settings.data.clock,
        deadlineAt: deadline ? deadline.getTime() : null,
        isOwed: isOwedPairing || isOnMove,
        isOwedPairing,
        opponents: seats
          .filter((seat) => !ownSlots.has(seat.slotIndex))
          .map((seat) =>
            isPending
              ? {
                  slotIndex: seat.slotIndex,
                  userId: null,
                  userName: HIDDEN_OPPONENT_NAME,
                  factionId: seat.factionId,
                  coId: null,
                  isHidden: true,
                }
              : {
                  slotIndex: seat.slotIndex,
                  userId: seat.userId,
                  userName:
                    seat.userName ??
                    (seat.aiProfileId === null ? "Unknown" : aiSeatName(seat.aiProfileId)),
                  factionId: seat.factionId,
                  coId: seat.coId,
                  isHidden: false,
                },
          ),
      },
    ];
  });

  // The pairing owed leads; it carries the one red key on the page.
  return ongoing.sort((left, right) => Number(right.isOwedPairing) - Number(left.isOwedPairing));
}

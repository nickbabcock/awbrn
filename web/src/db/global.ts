import { sql } from "drizzle-orm";
import {
  check,
  foreignKey,
  index,
  integer,
  primaryKey,
  real,
  sqliteTable,
  text,
  uniqueIndex,
} from "drizzle-orm/sqlite-core";
import { aiProfileIds, matchOutcomes } from "#/matches/schemas.ts";
import { rankedPools } from "#/matches/schemas.ts";
import { pairingStatuses } from "#/matches/schemas.ts";
import type {
  AiProfileId,
  MatchOutcome,
  MatchPhase,
  MatchSettings,
  RankedPool,
  PairingStatus,
  SeatResultReason,
} from "#/matches/schemas.ts";
import { MAP_RANKS, MAP_TAGS } from "#/maps/schemas.ts";
import { MODERATION_ACTIONS, MODERATION_SUBJECTS } from "#/moderation/schemas.ts";
import type {
  ModerationAction,
  ModerationDetails,
  ModerationSubject,
} from "#/moderation/schemas.ts";
import type { MapRank, MapSourceKind, MapTag } from "#/maps/schemas.ts";

const sqlLiterals = (values: readonly string[]) => sql.raw(values.map((v) => `'${v}'`).join(", "));

export const user = sqliteTable("user", {
  id: text("id").primaryKey(),
  name: text("name").notNull(),
  email: text("email").notNull().unique(),
  emailVerified: integer("emailVerified", { mode: "boolean" }).notNull(),
  image: text("image"),

  /**
   * Which roles this user holds, as a comma separated list, or null while
   * they hold the default. The vocabulary lives in `auth/access.ts` and is
   * not checked here: the admin plugin writes a list, so a column check that
   * named one role would reject a user who holds two.
   */
  role: text("role"),
  banned: integer("banned", { mode: "boolean" }).default(false),
  banReason: text("banReason"),
  banExpires: integer("banExpires", { mode: "timestamp" }),

  createdAt: integer("createdAt", { mode: "timestamp" })
    .notNull()
    .default(sql`(unixepoch())`),
  updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull(),
});

export const session = sqliteTable(
  "session",
  {
    id: text("id").primaryKey(),
    expiresAt: integer("expiresAt", { mode: "timestamp" }).notNull(),
    token: text("token").notNull().unique(),
    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
    updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull(),
    ipAddress: text("ipAddress"),
    userAgent: text("userAgent"),
    userId: text("userId")
      .notNull()
      .references(() => user.id, { onDelete: "cascade" }),
    /** The admin who is acting as this user, while an impersonation runs. */
    impersonatedBy: text("impersonatedBy"),
  },
  (t) => [index("session_userId_idx").on(t.userId)],
);

export const account = sqliteTable(
  "account",
  {
    id: text("id").primaryKey(),
    accountId: text("accountId").notNull(),
    providerId: text("providerId").notNull(),
    issuer: text("issuer").notNull(),
    userId: text("userId")
      .notNull()
      .references(() => user.id, { onDelete: "cascade" }),
    accessToken: text("accessToken"),
    refreshToken: text("refreshToken"),
    idToken: text("idToken"),
    accessTokenExpiresAt: integer("accessTokenExpiresAt", { mode: "timestamp" }),
    refreshTokenExpiresAt: integer("refreshTokenExpiresAt", { mode: "timestamp" }),
    scope: text("scope"),
    password: text("password"),
    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
    updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull(),
  },
  (t) => [
    index("account_userId_idx").on(t.userId),
    uniqueIndex("account_issuer_accountId_uidx").on(t.issuer, t.accountId),
  ],
);

export const verification = sqliteTable(
  "verification",
  {
    id: text("id").primaryKey(),
    identifier: text("identifier").notNull(),
    value: text("value").notNull(),
    expiresAt: integer("expiresAt", { mode: "timestamp" }).notNull(),
    createdAt: integer("createdAt", { mode: "timestamp" }).default(sql`(unixepoch())`),
    updatedAt: integer("updatedAt", { mode: "timestamp" }),
  },
  (t) => [index("verification_identifier_idx").on(t.identifier)],
);

export const matches = sqliteTable(
  "matches",
  {
    id: text("id").primaryKey(),
    name: text("name").notNull(),
    phase: text("phase").notNull().$type<MatchPhase>(),
    creatorUserId: text("creatorUserId")
      .notNull()
      .references(() => user.id, { onDelete: "restrict" }),
    mapId: text("mapId")
      .notNull()
      .references(() => maps.id, { onDelete: "restrict" }),
    mapRevision: integer("mapRevision").notNull(),
    maxPlayers: integer("maxPlayers").notNull(),
    isPrivate: integer("isPrivate", { mode: "boolean" }).notNull(),
    joinSlug: text("joinSlug"),
    settings: text("settings", { mode: "json" }).$type<MatchSettings>().notNull(),
    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
    updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull(),
    startedAt: integer("startedAt", { mode: "timestamp" }),
    completedAt: integer("completedAt", { mode: "timestamp" }),

    /**
     * The seat on the move, or null while no turn is open.
     *
     * Whose turn it is belongs to the match durable object, which derives it
     * from its event log. This column is the durable object's report of it,
     * written at each turn boundary, because a badge that counts the matches
     * waiting on a player is a query over every match and cannot wake one
     * durable object for each.
     */
    activeSlotIndex: integer("activeSlotIndex"),
    /** When the active seat runs out. Written with `activeSlotIndex`. */
    turnDeadlineAt: integer("turnDeadlineAt", { mode: "timestamp" }),
    pool: text("pool").$type<RankedPool>(),
    season: integer("season").references(() => seasons.number, { onDelete: "restrict" }),
  },
  (t) => [
    foreignKey({
      columns: [t.mapId, t.mapRevision],
      foreignColumns: [mapRevisions.mapId, mapRevisions.revision],
    }).onDelete("restrict"),
    index("matches_creator_idx").on(t.creatorUserId),
    index("matches_browse_idx").on(t.phase, t.isPrivate, t.createdAt),
    uniqueIndex("matches_joinSlug_unique").on(t.joinSlug),
    index("matches_ranked_active_idx")
      .on(t.pool, t.phase)
      .where(sql`${t.pool} is not null`),
    index("matches_active_turn_idx")
      .on(t.activeSlotIndex)
      .where(sql`${t.phase} = 'active' and ${t.activeSlotIndex} is not null`),
    check(
      "matches_ranked_identity_complete",
      sql`(${t.pool} is null and ${t.season} is null) or (${t.pool} is not null and ${t.season} is not null)`,
    ),
    check(
      "matches_pool_vocabulary",
      sql`${t.pool} is null or ${t.pool} in (${sqlLiterals(rankedPools)})`,
    ),
  ],
);

/**
 * One seat of a match, and who holds it.
 *
 * A seat carries a `userId` or an `aiProfileId`, never both and never neither.
 * That is what makes the occupant a fact of the row rather than a convention
 * the queries have to keep: everything that means "a person" already asks for
 * `userId`, and a seat the server plays does not have one.
 */
export const matchParticipants = sqliteTable(
  "match_participants",
  {
    matchId: text("matchId")
      .notNull()
      .references(() => matches.id, { onDelete: "cascade" }),
    /** The person holding this seat. Null when the server plays it. */
    userId: text("userId").references(() => user.id, { onDelete: "restrict" }),
    /**
     * The opponent the server plays this seat as. Null when a person holds it.
     *
     * A versioned engine profile id and not a difficulty word, so a finished
     * match records which opponent it was against even after the tier that
     * offered it is retuned.
     */
    aiProfileId: text("aiProfileId").$type<AiProfileId>(),
    slotIndex: integer("slotIndex").notNull(),
    factionId: integer("factionId").notNull(),
    coId: integer("coId"),
    ready: integer("ready", { mode: "boolean" }).notNull(),
    joinedAt: integer("joinedAt", { mode: "timestamp" }).notNull(),
    updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull(),
  },
  (t) => [
    primaryKey({ columns: [t.matchId, t.slotIndex] }),
    index("match_participants_match_idx").on(t.matchId),
    index("match_participants_match_user_idx").on(t.matchId, t.userId),
    index("match_participants_user_match_idx").on(t.userId, t.matchId),
    check(
      "match_participants_one_occupant",
      sql`(${t.userId} is null) <> (${t.aiProfileId} is null)`,
    ),
    check(
      "match_participants_ai_vocabulary",
      sql`${t.aiProfileId} is null or ${t.aiProfileId} in (${sqlLiterals(aiProfileIds)})`,
    ),
  ],
);

/**
 * One authoritative result row per seat. `reason` stores an elimination cause
 * or match ending. A null reason means a standing winner. `outcome` is the
 * team result, `placement` is the final rank, and `pool` marks ranked play.
 *
 * A seat the server played is recorded like any other, holding an
 * `aiProfileId` where a person's seat holds a `userId`. The match happened and
 * the record says so; what it never carries is a pool, because a rating is
 * between people.
 */
export const matchResults = sqliteTable(
  "match_results",
  {
    matchId: text("matchId")
      .notNull()
      .references(() => matches.id, { onDelete: "cascade" }),
    slotIndex: integer("slotIndex").notNull(),
    /** The person who held this seat. Null when the server played it. */
    userId: text("userId").references(() => user.id, { onDelete: "restrict" }),
    /** The opponent that held it. Null when a person did. */
    aiProfileId: text("aiProfileId").$type<AiProfileId>(),
    teamId: text("teamId"),
    outcome: text("outcome").notNull().$type<MatchOutcome>(),
    placement: integer("placement").notNull(),
    reason: text("reason").$type<SeatResultReason>(),
    pool: text("pool").$type<RankedPool>(),
    recordedAt: integer("recordedAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
  },
  (t) => [
    primaryKey({ columns: [t.matchId, t.slotIndex] }),
    index("match_results_user_idx").on(t.userId, t.recordedAt),
    index("match_results_pool_idx")
      .on(t.pool, t.recordedAt)
      .where(sql`${t.pool} is not null`),
    check(
      "match_results_placement_matches_outcome",
      // SQLite stores fractional values as REAL. Reject them before checking rank.
      sql`typeof(${t.placement}) = 'integer' and ${t.placement} >= 1 and (${t.placement} = 1) = (${t.outcome} in ('win', 'draw'))`,
    ),
    check("match_results_outcome_vocabulary", sql`${t.outcome} in (${sqlLiterals(matchOutcomes)})`),
    check("match_results_one_occupant", sql`(${t.userId} is null) <> (${t.aiProfileId} is null)`),
    check(
      "match_results_ai_vocabulary",
      sql`${t.aiProfileId} is null or ${t.aiProfileId} in (${sqlLiterals(aiProfileIds)})`,
    ),
    // A match the server took a seat in is not ranked, so it never carries a
    // pool. Ratings are between people.
    check("match_results_ai_is_never_ranked", sql`${t.aiProfileId} is null or ${t.pool} is null`),
    check(
      "match_results_reason_null_only_for_standing_win",
      sql`${t.reason} is not null or ${t.outcome} = 'win'`,
    ),
  ],
);

/**
 * Records a voided match without changing its result.
 *
 * This is the state and not the record of who made it: a void can come from
 * a moderator or from the server itself, and only the first of those has a
 * person to name. `moderation_actions` holds the person, the reason they
 * wrote for the record, and the time.
 */
export const matchVoids = sqliteTable(
  "match_voids",
  {
    matchId: text("matchId")
      .primaryKey()
      .references(() => matches.id, { onDelete: "cascade" }),
    /** What the players in the match are told. Free text in the first release. */
    publicReason: text("publicReason").notNull(),
    voidedAt: integer("voidedAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
  },
  (t) => [index("match_voids_voidedAt_idx").on(t.voidedAt)],
);

/** One logical map; external identity lives in `mapSources`. */
export const maps = sqliteTable(
  "maps",
  {
    id: text("id").primaryKey(),

    // Editable metadata; changes do not create revisions.
    name: text("name").notNull(),
    author: text("author").notNull(),
    authorUserId: text("authorUserId").references(() => user.id, { onDelete: "set null" }),

    /** Current revision; maintained with `map_revisions`. */
    currentRevision: integer("currentRevision").notNull(),

    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
    updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull(),
  },
  (t) => [index("maps_author_idx").on(t.authorUserId)],
);

/** Optional external identity for a map. */
export const mapSources = sqliteTable(
  "map_sources",
  {
    mapId: text("mapId")
      .primaryKey()
      .references(() => maps.id, { onDelete: "cascade" }),
    source: text("source").notNull().$type<MapSourceKind>(),
    sourceMapId: integer("sourceMapId").notNull(),
  },
  (t) => [uniqueIndex("map_sources_source_unique").on(t.source, t.sourceMapId)],
);

/** Immutable playable content for a map revision. */
export const mapRevisions = sqliteTable(
  "map_revisions",
  {
    mapId: text("mapId")
      .notNull()
      .references(() => maps.id, { onDelete: "cascade" }),
    revision: integer("revision").notNull(),

    contentHash: text("contentHash").notNull(),

    width: integer("width").notNull(),
    height: integer("height").notNull(),
    playerCount: integer("playerCount").notNull(),

    // Replay-matching signatures.
    propertySignature: text("propertySignature").notNull(),
    unitSignature: text("unitSignature").notNull(),

    /** Quality of this content, from C to S. Null while it is unranked. */
    rank: text("rank").$type<MapRank>(),

    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
    lastSeenAt: integer("lastSeenAt", { mode: "timestamp" }),
  },
  (t) => [
    primaryKey({ columns: [t.mapId, t.revision] }),
    uniqueIndex("map_revisions_content_unique").on(t.mapId, t.contentHash),
    index("map_revisions_signature_idx").on(t.mapId, t.propertySignature),
    check(
      "map_revisions_rank_vocabulary",
      sql`${t.rank} is null or ${t.rank} in (${sqlLiterals(MAP_RANKS)})`,
    ),
  ],
);

/**
 * How a map plays: one row per tag it carries.
 *
 * Tags describe the map and not one of its revisions, so a new revision keeps
 * them while it loses the rank of the revision before it.
 */
export const mapTags = sqliteTable(
  "map_tags",
  {
    mapId: text("mapId")
      .notNull()
      .references(() => maps.id, { onDelete: "cascade" }),
    tag: text("tag").notNull().$type<MapTag>(),
    addedAt: integer("addedAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
  },
  (t) => [
    primaryKey({ columns: [t.mapId, t.tag] }),
    index("map_tags_tag_idx").on(t.tag, t.mapId),
    check("map_tags_vocabulary", sql`${t.tag} in (${sqlLiterals(MAP_TAGS)})`),
  ],
);

/** One ranked-play season. */
export const seasons = sqliteTable(
  "seasons",
  {
    number: integer("number").primaryKey(),
    startsAt: integer("startsAt", { mode: "timestamp" }).notNull(),
    endsAt: integer("endsAt", { mode: "timestamp" }).notNull(),
  },
  (t) => [check("seasons_dates_ordered", sql`${t.startsAt} < ${t.endsAt}`)],
);

/** The curated maps that a ranked pool may select for a season. */
export const rankedMaps = sqliteTable(
  "ranked_maps",
  {
    season: integer("season")
      .notNull()
      .references(() => seasons.number, { onDelete: "restrict" }),
    pool: text("pool").notNull().$type<RankedPool>(),
    mapId: text("mapId")
      .notNull()
      .references(() => maps.id, { onDelete: "restrict" }),
    addedAt: integer("addedAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
  },
  (t) => [
    primaryKey({ columns: [t.season, t.pool, t.mapId] }),
    index("ranked_maps_pool_idx").on(t.season, t.pool),
    check("ranked_maps_pool_vocabulary", sql`${t.pool} in (${sqlLiterals(rankedPools)})`),
  ],
);

/** A persistent request for ranked matches in one pool. */
export const seeks = sqliteTable(
  "seeks",
  {
    userId: text("userId")
      .notNull()
      .references(() => user.id, { onDelete: "cascade" }),
    pool: text("pool").notNull().$type<RankedPool>(),
    generation: text("generation").notNull(),
    maxActiveMatches: integer("maxActiveMatches").notNull(),
    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
  },
  (t) => [
    primaryKey({ columns: [t.userId, t.pool] }),
    index("seeks_pool_created_idx").on(t.pool, t.createdAt),
    check("seeks_pool_vocabulary", sql`${t.pool} in (${sqlLiterals(rankedPools)})`),
    check(
      "seeks_active_match_limit",
      sql`typeof(${t.maxActiveMatches}) = 'integer' and ${t.maxActiveMatches} between 1 and 5`,
    ),
  ],
);

/** The current Glicko-2 state for one user in one ranked pool. */
export const ratings = sqliteTable(
  "ratings",
  {
    userId: text("userId")
      .notNull()
      .references(() => user.id, { onDelete: "cascade" }),
    pool: text("pool").notNull().$type<RankedPool>(),
    rating: real("rating").notNull().default(1500),
    deviation: real("deviation").notNull().default(350),
    volatility: real("volatility").notNull().default(0.06),
    lastRatedAt: integer("lastRatedAt", { mode: "timestamp" }),
    ratedMatches: integer("ratedMatches").notNull().default(0),
  },
  (t) => [
    primaryKey({ columns: [t.userId, t.pool] }),
    index("ratings_pool_rating_idx").on(t.pool, t.rating),
    check("ratings_pool_vocabulary", sql`${t.pool} in (${sqlLiterals(rankedPools)})`),
    check("ratings_deviation_positive", sql`${t.deviation} > 0`),
    check("ratings_volatility_positive", sql`${t.volatility} > 0`),
    check("ratings_match_count_nonnegative", sql`${t.ratedMatches} >= 0`),
  ],
);

/** The durable audit record for a ranked pairing and its confirmation. */
export const pairings = sqliteTable(
  "pairings",
  {
    id: text("id").primaryKey(),
    matchId: text("matchId")
      .notNull()
      .unique()
      .references(() => matches.id, { onDelete: "restrict" }),
    pool: text("pool").notNull().$type<RankedPool>(),
    season: integer("season")
      .notNull()
      .references(() => seasons.number, { onDelete: "restrict" }),
    userOneId: text("userOneId")
      .notNull()
      .references(() => user.id, { onDelete: "restrict" }),
    userTwoId: text("userTwoId")
      .notNull()
      .references(() => user.id, { onDelete: "restrict" }),
    userOneSeekGeneration: text("userOneSeekGeneration").notNull(),
    userTwoSeekGeneration: text("userTwoSeekGeneration").notNull(),
    status: text("status").notNull().$type<PairingStatus>(),
    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
    deadlineAt: integer("deadlineAt", { mode: "timestamp" }).notNull(),
    resolvedAt: integer("resolvedAt", { mode: "timestamp" }),
  },
  (t) => [
    index("pairings_pending_deadline_idx")
      .on(t.pool, t.season, t.deadlineAt)
      .where(sql`${t.status} = 'pending'`),
    index("pairings_users_idx").on(t.pool, t.userOneId, t.userTwoId, t.createdAt),
    check("pairings_users_ordered", sql`${t.userOneId} < ${t.userTwoId}`),
    check("pairings_pool_vocabulary", sql`${t.pool} in (${sqlLiterals(rankedPools)})`),
    check("pairings_status_vocabulary", sql`${t.status} in (${sqlLiterals(pairingStatuses)})`),
  ],
);

/**
 * Every act of moderation, appended and never changed.
 *
 * This is the record and not the state. A screen that asks whether a match is
 * void reads `match_voids`; a screen that asks who voided it, when, and why
 * reads this table. Game code never reads it.
 *
 * `subjectId` names a row in whichever table `subjectType` points at, so it
 * carries no foreign key and can outlive the row it names. That is the price
 * of one table that answers "everything this moderator did" with a single
 * index scan instead of a union over one table for each power.
 */
export const moderationActions = sqliteTable(
  "moderation_actions",
  {
    id: text("id").primaryKey(),
    actorUserId: text("actorUserId")
      .notNull()
      .references(() => user.id, { onDelete: "restrict" }),
    action: text("action").notNull().$type<ModerationAction>(),
    subjectType: text("subjectType").notNull().$type<ModerationSubject>(),
    subjectId: text("subjectId").notNull(),
    /** Why the moderator acted, for the record. Never shown to the subject. */
    reason: text("reason").notNull(),
    /** What changed, such as the tags before and after. */
    details: text("details", { mode: "json" }).$type<ModerationDetails>(),
    createdAt: integer("createdAt", { mode: "timestamp" })
      .notNull()
      .default(sql`(unixepoch())`),
  },
  (t) => [
    index("moderation_actions_subject_idx").on(t.subjectType, t.subjectId, t.createdAt),
    index("moderation_actions_actor_idx").on(t.actorUserId, t.createdAt),
    index("moderation_actions_recent_idx").on(t.createdAt),
    check(
      "moderation_actions_action_vocabulary",
      sql`${t.action} in (${sqlLiterals(MODERATION_ACTIONS)})`,
    ),
    check(
      "moderation_actions_subject_vocabulary",
      sql`${t.subjectType} in (${sqlLiterals(MODERATION_SUBJECTS)})`,
    ),
  ],
);

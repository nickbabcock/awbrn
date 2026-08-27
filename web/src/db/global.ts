import { sql } from "drizzle-orm";
import {
  check,
  foreignKey,
  index,
  integer,
  primaryKey,
  sqliteTable,
  text,
  uniqueIndex,
} from "drizzle-orm/sqlite-core";
import { matchOutcomes } from "#/matches/schemas.ts";
import type {
  MatchOutcome,
  MatchPhase,
  MatchSettings,
  RankedPool,
  SeatResultReason,
} from "#/matches/schemas.ts";
import { MAP_RANKS, MAP_TAGS } from "#/maps/schemas.ts";
import type { MapRank, MapSourceKind, MapTag } from "#/maps/schemas.ts";

const sqlLiterals = (values: readonly string[]) => sql.raw(values.map((v) => `'${v}'`).join(", "));

export const user = sqliteTable("user", {
  id: text("id").primaryKey(),
  name: text("name").notNull(),
  email: text("email").notNull().unique(),
  emailVerified: integer("emailVerified", { mode: "boolean" }).notNull(),
  image: text("image"),
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
  },
  (t) => [
    foreignKey({
      columns: [t.mapId, t.mapRevision],
      foreignColumns: [mapRevisions.mapId, mapRevisions.revision],
    }).onDelete("restrict"),
    index("matches_creator_idx").on(t.creatorUserId),
    index("matches_browse_idx").on(t.phase, t.isPrivate, t.createdAt),
    uniqueIndex("matches_joinSlug_unique").on(t.joinSlug),
  ],
);

export const matchParticipants = sqliteTable(
  "match_participants",
  {
    matchId: text("matchId")
      .notNull()
      .references(() => matches.id, { onDelete: "cascade" }),
    userId: text("userId")
      .notNull()
      .references(() => user.id, { onDelete: "restrict" }),
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
  ],
);

/**
 * One authoritative result row per seat. `reason` stores an elimination cause
 * or match ending. A null reason means a standing winner. `outcome` is the
 * team result, `placement` is the final rank, and `pool` marks ranked play.
 */
export const matchResults = sqliteTable(
  "match_results",
  {
    matchId: text("matchId")
      .notNull()
      .references(() => matches.id, { onDelete: "cascade" }),
    slotIndex: integer("slotIndex").notNull(),
    userId: text("userId")
      .notNull()
      .references(() => user.id, { onDelete: "restrict" }),
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
    check(
      "match_results_reason_null_only_for_standing_win",
      sql`${t.reason} is not null or ${t.outcome} = 'win'`,
    ),
  ],
);

/** Records a voided match without changing its result. */
export const matchVoids = sqliteTable(
  "match_voids",
  {
    matchId: text("matchId")
      .primaryKey()
      .references(() => matches.id, { onDelete: "cascade" }),
    voidedByUserId: text("voidedByUserId")
      .notNull()
      .references(() => user.id, { onDelete: "restrict" }),
    /** Free text for abuse records in the first release. */
    reason: text("reason").notNull(),
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

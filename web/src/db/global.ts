import { sql } from "drizzle-orm";
import {
  index,
  integer,
  primaryKey,
  sqliteTable,
  text,
  uniqueIndex,
} from "drizzle-orm/sqlite-core";
import type { MatchPhase, MatchSettings } from "#/matches/schemas.ts";

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
  (t) => [index("account_userId_idx").on(t.userId)],
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
    mapId: integer("mapId").notNull(),
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
    primaryKey({ columns: [t.matchId, t.userId] }),
    uniqueIndex("match_participants_match_slot_unique").on(t.matchId, t.slotIndex),
    index("match_participants_match_idx").on(t.matchId),
  ],
);

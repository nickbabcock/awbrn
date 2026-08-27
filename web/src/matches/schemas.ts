import { z } from "zod";
import { coRoster, isKnownCoId } from "#/co_roster.ts";
import { awbrnMapDocumentSchema } from "#/maps/map_document.ts";
import { MAP_ID_LENGTH } from "#/maps/map_id.ts";
import { mapRefSchema } from "#/maps/schemas.ts";
import { moderationReasonSchema } from "#/moderation/schemas.ts";
import { matchIdSchema } from "./match_id.ts";

/**
 * The COs no seat in the match may take.
 *
 * The host sets the list once, when the lobby is opened, and it does not
 * change after that: a ban that could arrive after a player picked would take
 * a CO out from under them. Ids are written once each and in ascending order,
 * so two hosts who ban the same COs store the same setting.
 *
 * A ban list that names every CO would leave a lobby nobody can ready in, so
 * one CO always survives it.
 */
export const bannedCoIdsSchema = z
  .array(z.number().int().positive())
  .transform((ids) => [...new Set(ids)].filter(isKnownCoId).sort((left, right) => left - right))
  .refine(
    (ids) => ids.length < coRoster.length,
    "at least one CO has to be left for the players to choose",
  );

export const matchSettingsSchema = z.object({
  fogEnabled: z.boolean(),
  startingFunds: z.number().int().nonnegative(),
  hotseatEnabled: z.boolean().default(false),
  // Matches created before COs could be banned have no list, and read as a
  // match where nothing is banned.
  bannedCoIds: bannedCoIdsSchema.default([]),
});

export const matchCreateRequestSchema = z.object({
  name: z
    .string()
    .refine((s) => s.trim().length > 0, "match name is required")
    .transform((s) => s.trim()),
  map: mapRefSchema,
  isPrivate: z.boolean(),
  settings: matchSettingsSchema,
});

export const matchBrowseRequestSchema = z.object({
  cursor: z.string().min(1).optional(),
});

export const matchHistoryRequestSchema = z.object({
  cursor: z.string().min(1).optional(),
});

export const matchMutationRequestSchema = z.discriminatedUnion("action", [
  z.object({
    action: z.literal("join"),
    slotIndex: z.number().int().nonnegative(),
    factionId: z.number().int(),
    joinSlug: z.string().nullable().optional(),
  }),
  z.object({ action: z.literal("leave"), slotIndex: z.number().int().nonnegative() }),
  z.object({
    action: z.literal("updateParticipant"),
    slotIndex: z.number().int().nonnegative(),
    factionId: z.number().int().optional(),
    coId: z.number().int().positive().nullable().optional(),
    ready: z.boolean().optional(),
    joinSlug: z.string().nullable().optional(),
  }),
]);

export type MatchPhase = "draft" | "lobby" | "starting" | "active" | "completed" | "cancelled";

/** Engine reasons for a seat elimination or match ending. */
export const seatResultReasons = [
  "rout",
  "hq-capture",
  "lab-capture",
  "capture-limit",
  "day-limit",
  "resignation",
  "timeout",
  "agreement",
] as const;

export const seatResultReasonSchema = z.enum(seatResultReasons);

/** Result for one seat. Team members share the team outcome. */
export const matchOutcomes = ["win", "loss", "draw"] as const;

export const matchOutcomeSchema = z.enum(matchOutcomes);

/** Terminal seat status derived from its result reason. */
export const matchSeatStatuses = ["active", "resigned", "timed-out", "eliminated"] as const;

export const matchSeatStatusSchema = z.enum(matchSeatStatuses);

/** Ranked pools, one per `{ fog, pace }` pair. Live pools open after async ones. */
export const rankedPools = ["async", "fog_async", "live", "fog_live"] as const;

export const rankedPoolSchema = z.enum(rankedPools);

export type SeatResultReason = (typeof seatResultReasons)[number];
export type MatchOutcome = (typeof matchOutcomes)[number];
export type MatchSeatStatus = (typeof matchSeatStatuses)[number];
export type RankedPool = (typeof rankedPools)[number];
export type MatchSettings = z.infer<typeof matchSettingsSchema>;
export type MatchCreateRequest = z.infer<typeof matchCreateRequestSchema>;
export type MatchBrowseRequest = z.infer<typeof matchBrowseRequestSchema>;
export type MatchHistoryRequest = z.infer<typeof matchHistoryRequestSchema>;
export type MatchMutationRequest = z.infer<typeof matchMutationRequestSchema>;

export interface MatchCreateResponse {
  matchId: string;
  joinSlug: string | null;
}

export interface MatchBrowseSummary {
  matchId: string;
  name: string;
  creatorName: string;
  mapId: string;
  mapRevision: number;
  maxPlayers: number;
  participantCount: number;
  openSlotCount: number;
  joinedPlayerNames: string[];
  settings: MatchSettings;
  createdAt: string;
}

export interface MatchBrowseResponse {
  matches: MatchBrowseSummary[];
  pageSize: number;
  hasNextPage: boolean;
  nextCursor: string | null;
}

export interface MyMatchParticipantSummary {
  slotIndex: number;
  factionId: number;
  coId: number | null;
  ready: boolean;
  joinedAt: string;
  updatedAt: string;
}

export interface MyMatchSummary {
  matchId: string;
  name: string;
  phase: MatchPhase;
  creatorName: string;
  mapId: string;
  mapRevision: number;
  maxPlayers: number;
  participantCount: number;
  openSlotCount: number;
  isPrivate: boolean;
  settings: MatchSettings;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  viewerParticipants: MyMatchParticipantSummary[];
}

export interface MyMatchesResponse {
  matches: MyMatchSummary[];
}

export interface MatchParticipantSnapshot {
  userId: string;
  userName: string;
  slotIndex: number;
  factionId: number;
  coId: number | null;
  ready: boolean;
  joinedAt: string;
  updatedAt: string;
}

export interface MatchSnapshot {
  matchId: string;
  name: string;
  phase: MatchPhase;
  creatorUserId: string;
  creatorName: string;
  mapId: string;
  mapRevision: number;
  maxPlayers: number;
  isPrivate: boolean;
  joinSlug: string | null;
  settings: MatchSettings;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
  participants: MatchParticipantSnapshot[];
  /**
   * Set when the match does not count, with the reason the players are told.
   * Who voided it and why they did is in the moderation log, not here.
   */
  void: MatchVoidSnapshot | null;
}

export interface MatchVoidSnapshot {
  publicReason: string;
  voidedAt: string;
}

/** Void a match: one reason for the players, one for the record. */
export const matchVoidRequestSchema = z.object({
  matchId: matchIdSchema,
  publicReason: z.string().trim().min(3).max(200),
  reason: moderationReasonSchema,
});

export type MatchVoidRequest = z.infer<typeof matchVoidRequestSchema>;

export interface MatchMutationResponse {
  match: MatchSnapshot;
}

export const matchSetupPlayerSchema = z.object({
  userId: z.string(),
  factionId: z.number().int(),
  team: z.null(),
  startingFunds: z.number().int().nonnegative(),
  coId: z.number().int(),
});

export const matchSetupSchema = z.object({
  matchId: matchIdSchema,
  mapId: z.string().length(MAP_ID_LENGTH),
  revision: z.number().int().positive(),
  map: awbrnMapDocumentSchema,
  players: z.array(matchSetupPlayerSchema),
  fogEnabled: z.boolean(),
  startingFunds: z.number().int().nonnegative(),
  creatorUserId: z.string(),
});

export type MatchSetupPlayer = z.infer<typeof matchSetupPlayerSchema>;
export type MatchSetup = z.infer<typeof matchSetupSchema>;

/** One seat in a finished match, with the result recorded for it. */
export interface MatchHistorySeat {
  slotIndex: number;
  userId: string;
  userName: string;
  factionId: number;
  coId: number | null;
  /** Null while a completed match has no recorded result for the seat. */
  outcome: MatchOutcome | null;
  placement: number | null;
  reason: SeatResultReason | null;
}

/** One finished match, as the viewer's own after action report. */
export interface MatchHistoryEntry {
  matchId: string;
  name: string;
  mapId: string;
  mapRevision: number;
  awbwMapId: number | null;
  isPrivate: boolean;
  settings: MatchSettings;
  startedAt: string | null;
  completedAt: string;
  /** Every seat the viewer held. More than one means a hotseat match. */
  viewerSlotIndexes: number[];
  seats: MatchHistorySeat[];
  /** False when the archive is missing, so the page never offers a dead file. */
  hasReplay: boolean;
}

export interface MatchHistoryResponse {
  matches: MatchHistoryEntry[];
  pageSize: number;
  hasNextPage: boolean;
  nextCursor: string | null;
}

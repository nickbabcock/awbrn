import { z } from "zod";
import { awbrnMapDocumentSchema } from "#/maps/map_document.ts";
import { MAP_ID_LENGTH } from "#/maps/map_id.ts";
import { matchIdSchema } from "./match_id.ts";

export const matchSettingsSchema = z.object({
  fogEnabled: z.boolean(),
  startingFunds: z.number().int().nonnegative(),
  hotseatEnabled: z.boolean().default(false),
});

export const matchCreateRequestSchema = z.object({
  name: z
    .string()
    .refine((s) => s.trim().length > 0, "match name is required")
    .transform((s) => s.trim()),
  mapId: z.number().int().positive(),
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
}

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

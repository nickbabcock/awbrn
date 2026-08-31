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

const MINUTE_MS = 60_000;
const DAY_MS = 24 * 60 * MINUTE_MS;

/** The longest bank or increment a host may set. */
export const MAX_CLOCK_MS = 30 * DAY_MS;

/**
 * The clock every new match runs on, in milliseconds.
 *
 * Each seat holds a bank that counts down only while its own turn is open.
 * Ending a turn adds `incrementMs` to what is left, up to `maxBankMs`. A seat
 * whose bank reaches zero is removed from the match, which is what stops a
 * match from running forever.
 */
export const matchClockSchema = z
  .object({
    initialMs: z.number().int().min(MINUTE_MS).max(MAX_CLOCK_MS),
    incrementMs: z.number().int().nonnegative().max(MAX_CLOCK_MS),
    maxBankMs: z.number().int().min(MINUTE_MS).max(MAX_CLOCK_MS),
  })
  .refine(
    (clock) => clock.maxBankMs >= clock.initialMs,
    "the bank ceiling cannot be below the starting time",
  );

/** What a host gets when they do not touch the clock: 7 days, +2 a turn. */
export const defaultMatchClock: MatchClock = {
  initialMs: 7 * DAY_MS,
  incrementMs: 2 * DAY_MS,
  maxBankMs: 7 * DAY_MS,
};

/**
 * The opponents a match may seat, easiest first.
 *
 * These are the engine's own profile identifiers and not difficulty words. A
 * retuned tier mints the next version rather than changing what an old record
 * means, so a finished match always says which opponent it was against.
 * `ai_profiles.test.ts` holds this list to the engine roster.
 */
export const aiProfileIds = ["ai-easy-v1", "ai-standard-v1", "ai-hard-v1"] as const;

export const aiProfileIdSchema = z.enum(aiProfileIds);

export type AiProfileId = (typeof aiProfileIds)[number];

/**
 * Who holds a seat.
 *
 * A seat is held by an occupant, and a person is one kind of occupant. Saying
 * it this way is what keeps a rating, a ban and a moderation record about
 * people: a query that means "a person" names `userId` and an opponent has
 * none, rather than every such query having to remember to exclude one.
 */
export const seatOccupantSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("human"), userId: z.string().min(1) }),
  z.object({ kind: z.literal("ai"), profileId: aiProfileIdSchema }),
]);

export type SeatOccupant = z.infer<typeof seatOccupantSchema>;

export const matchSettingsSchema = z.object({
  fogEnabled: z.boolean(),
  startingFunds: z.number().int().nonnegative(),
  hotseatEnabled: z.boolean().default(false),
  // Matches created before COs could be banned have no list, and read as a
  // match where nothing is banned.
  bannedCoIds: bannedCoIdsSchema.default([]),
  // Every match runs on a clock, so the settings always name one.
  clock: matchClockSchema,
});

/**
 * A seat the host fills as the match is made.
 *
 * Only an opponent can be seated here. A person claims a seat themselves, in
 * the lobby, which is the one place a seat learns whose it is.
 */
export const matchCreateSeatSchema = z.object({
  slotIndex: z.number().int().nonnegative(),
  profileId: aiProfileIdSchema,
});

export const matchCreateRequestSchema = z
  .object({
    name: z
      .string()
      .refine((s) => s.trim().length > 0, "match name is required")
      .transform((s) => s.trim()),
    map: mapRefSchema,
    isPrivate: z.boolean(),
    settings: matchSettingsSchema,
    /** Seats the server plays. Absent means a lobby of people. */
    aiSeats: z.array(matchCreateSeatSchema).default([]),
  })
  .refine(
    (input) => new Set(input.aiSeats.map((seat) => seat.slotIndex)).size === input.aiSeats.length,
    "a seat can only be filled once",
  );

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

export const rankedConfirmationRequestSchema = z.discriminatedUnion("action", [
  z.object({ action: z.literal("selectCommander"), coId: z.number().int().positive() }),
  z.object({ action: z.literal("ready") }),
  z.object({ action: z.literal("refuse") }),
]);

export type MatchPhase =
  | "draft"
  | "lobby"
  | "pending"
  | "starting"
  | "active"
  | "completed"
  | "cancelled";

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

/** The recorded outcome of a ranked confirmation window. */
export const pairingStatuses = ["pending", "confirmed", "expired", "refused"] as const;

export const pairingStatusSchema = z.enum(pairingStatuses);

export type SeatResultReason = (typeof seatResultReasons)[number];
export type MatchOutcome = (typeof matchOutcomes)[number];
export type MatchSeatStatus = (typeof matchSeatStatuses)[number];
export type RankedPool = (typeof rankedPools)[number];
export type PairingStatus = (typeof pairingStatuses)[number];
export type MatchClock = z.infer<typeof matchClockSchema>;
export type MatchSettings = z.infer<typeof matchSettingsSchema>;
export type MatchCreateRequest = z.infer<typeof matchCreateRequestSchema>;
export type MatchBrowseRequest = z.infer<typeof matchBrowseRequestSchema>;
export type MatchHistoryRequest = z.infer<typeof matchHistoryRequestSchema>;
export type MatchMutationRequest = z.infer<typeof matchMutationRequestSchema>;
export type RankedConfirmationRequest = z.infer<typeof rankedConfirmationRequestSchema>;

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
  /** The seat on the move, or null while no turn is open. */
  activeSlotIndex: number | null;
  /** When the active seat runs out, or null with no open turn. */
  turnDeadlineAt: string | null;
  viewerParticipants: MyMatchParticipantSummary[];
}

export interface MyMatchesResponse {
  matches: MyMatchSummary[];
}

/** How many of the viewer's matches are waiting on them to act. */
export interface MatchesAwaitingResponse {
  awaiting: number;
}

export interface MatchParticipantSnapshot {
  /** Null when the server plays this seat. */
  userId: string | null;
  aiProfileId: AiProfileId | null;
  /** The person's name, or the opponent's label. Always something to show. */
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
   * When the ranked confirmation window closes. Set only while the match is
   * pending, because no other phase has one.
   */
  confirmationDeadlineAt: string | null;
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
  /**
   * The person holding this seat, or null when the server plays it.
   *
   * The engine reads `aiProfileId` to decide which seats it owes turns to, so
   * these two are how a match knows the difference rather than a flag beside
   * them that could disagree.
   */
  userId: z.string().nullable().default(null),
  aiProfileId: aiProfileIdSchema.nullable().default(null),
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
  pool: rankedPoolSchema.nullable().default(null),
  season: z.number().int().positive().nullable().default(null),
  clock: matchClockSchema,
});

export type MatchSetupPlayer = z.infer<typeof matchSetupPlayerSchema>;
export type MatchSetup = z.input<typeof matchSetupSchema>;

/** One seat in a finished match, with the result recorded for it. */
export interface MatchHistorySeat {
  slotIndex: number;
  /** Null when the server played this seat. */
  userId: string | null;
  aiProfileId: AiProfileId | null;
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

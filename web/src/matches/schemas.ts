import { z } from "zod";

export const matchSettingsSchema = z.object({
  fogEnabled: z.boolean(),
  startingFunds: z.number().int().nonnegative(),
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

export const matchMutationRequestSchema = z.discriminatedUnion("action", [
  z.object({
    action: z.literal("join"),
    slotIndex: z.number().int().nonnegative(),
    factionId: z.number().int(),
    joinSlug: z.string().nullable().optional(),
  }),
  z.object({ action: z.literal("leave") }),
  z.object({
    action: z.literal("updateParticipant"),
    factionId: z.number().int().optional(),
    coId: z.number().int().positive().nullable().optional(),
    ready: z.boolean().optional(),
    joinSlug: z.string().nullable().optional(),
  }),
]);

export type MatchPhase = "draft" | "lobby" | "starting" | "active" | "completed" | "cancelled";
export type MatchSettings = z.infer<typeof matchSettingsSchema>;
export type MatchCreateRequest = z.infer<typeof matchCreateRequestSchema>;
export type MatchBrowseRequest = z.infer<typeof matchBrowseRequestSchema>;
export type MatchMutationRequest = z.infer<typeof matchMutationRequestSchema>;

export interface MatchCreateResponse {
  matchId: string;
  joinSlug: string | null;
}

export interface MatchBrowseSummary {
  matchId: string;
  name: string;
  creatorName: string;
  mapId: number;
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
  mapId: number;
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

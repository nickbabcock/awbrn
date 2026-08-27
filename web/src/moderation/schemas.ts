import { z } from "zod";
import { MODERATION_ID_LENGTH } from "./moderation_id.ts";

/**
 * Every act a moderator can take, named `subject.verb`.
 *
 * A name here is written to the log and read back on the moderation screen,
 * so a name that is retired stays in the vocabulary: the rows that hold it
 * are still true.
 */
export const MODERATION_ACTIONS = [
  "map.rank",
  "map.retag",
  "match.void",
  "user.ban",
  "user.unban",
  "user.set-role",
] as const;

export const moderationActionSchema = z.enum(MODERATION_ACTIONS);

export type ModerationAction = z.infer<typeof moderationActionSchema>;

/** What a logged act was taken against. */
export const MODERATION_SUBJECTS = ["map", "map_revision", "match", "user"] as const;

export const moderationSubjectSchema = z.enum(MODERATION_SUBJECTS);

export type ModerationSubject = z.infer<typeof moderationSubjectSchema>;

/**
 * What changed, as the act writes it.
 *
 * The shape is free because the moderation screen prints it and nothing
 * branches on it. Write what a person reading the row a year later needs:
 * the value before and the value after.
 */
export type ModerationDetails = Record<string, string | number | boolean | null | string[]>;

export const moderationIdSchema = z
  .string()
  .length(MODERATION_ID_LENGTH)
  .regex(/^[0-9a-z]+$/);

/** The shortest reason that is worth keeping, and the longest worth storing. */
export const MODERATION_REASON_MIN_LENGTH = 3;
export const MODERATION_REASON_MAX_LENGTH = 500;

export const moderationReasonSchema = z
  .string()
  .trim()
  .min(MODERATION_REASON_MIN_LENGTH)
  .max(MODERATION_REASON_MAX_LENGTH);

/** One row of the log, as a screen reads it. */
export interface ModerationLogEntry {
  id: string;
  actorUserId: string;
  actorName: string;
  action: ModerationAction;
  subjectType: ModerationSubject;
  subjectId: string;
  reason: string;
  details: ModerationDetails | null;
  createdAt: string;
}

/** How much of the log one page holds. */
export const MODERATION_LOG_PAGE_SIZE = 50;

export const moderationLogRequestSchema = z.object({
  subjectType: moderationSubjectSchema.optional(),
  subjectId: z.string().min(1).optional(),
  actorUserId: z.string().min(1).optional(),
  limit: z.number().int().positive().max(MODERATION_LOG_PAGE_SIZE).optional(),
});

export type ModerationLogRequest = z.infer<typeof moderationLogRequestSchema>;

import { env } from "cloudflare:workers";
import { and, desc, eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import type { Actor } from "#/auth/actor.ts";
import { moderationActions, user } from "#/db/global.ts";
import { generateModerationId } from "./moderation_id.ts";
import {
  MODERATION_LOG_PAGE_SIZE,
  type ModerationAction,
  type ModerationDetails,
  type ModerationLogEntry,
  type ModerationLogRequest,
  type ModerationSubject,
} from "./schemas.ts";

const db = drizzle(env.DB, { schema: { moderationActions, user } });

export interface ModerationEntryInput {
  actor: Actor;
  action: ModerationAction;
  subjectType: ModerationSubject;
  subjectId: string;
  reason: string;
  details?: ModerationDetails;
  now?: Date;
}

/**
 * The row that records an act, ready to insert.
 *
 * This builds values rather than writing them, so the caller puts the insert
 * in the same batch as the change it records. A batch on D1 is one
 * transaction: an act that lands without its record, or a record without its
 * act, is then not a state the database can reach.
 */
export function moderationEntry(
  input: ModerationEntryInput,
): typeof moderationActions.$inferInsert {
  return {
    id: generateModerationId(),
    actorUserId: input.actor.userId,
    action: input.action,
    subjectType: input.subjectType,
    subjectId: input.subjectId,
    reason: input.reason,
    details: input.details ?? null,
    createdAt: input.now ?? new Date(),
  };
}

/**
 * Record an act that changes nothing else in this database.
 *
 * Banning writes through the admin plugin and setting a role writes through
 * it too, so those two have no batch of ours to join.
 */
export async function logModeration(input: ModerationEntryInput): Promise<void> {
  await db.insert(moderationActions).values(moderationEntry(input));
}

/** The log, newest first, narrowed to a subject or to an actor. */
export async function listModerationActions(
  request: ModerationLogRequest = {},
): Promise<ModerationLogEntry[]> {
  const filters = [
    request.subjectType ? eq(moderationActions.subjectType, request.subjectType) : undefined,
    request.subjectId ? eq(moderationActions.subjectId, request.subjectId) : undefined,
    request.actorUserId ? eq(moderationActions.actorUserId, request.actorUserId) : undefined,
  ].filter((filter) => filter !== undefined);

  const rows = await db
    .select({
      id: moderationActions.id,
      actorUserId: moderationActions.actorUserId,
      actorName: user.name,
      action: moderationActions.action,
      subjectType: moderationActions.subjectType,
      subjectId: moderationActions.subjectId,
      reason: moderationActions.reason,
      details: moderationActions.details,
      createdAt: moderationActions.createdAt,
    })
    .from(moderationActions)
    .innerJoin(user, eq(user.id, moderationActions.actorUserId))
    .where(filters.length === 0 ? undefined : and(...filters))
    .orderBy(desc(moderationActions.createdAt), desc(moderationActions.id))
    .limit(request.limit ?? MODERATION_LOG_PAGE_SIZE)
    .all();

  return rows.map((row) => ({
    ...row,
    details: row.details ?? null,
    createdAt: row.createdAt.toISOString(),
  }));
}

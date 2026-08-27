import { createServerFn } from "@tanstack/react-start";
import { getRequest } from "@tanstack/react-start/server";
import { z } from "zod";
import { getAuth } from "#/auth/auth.server.ts";
import { requirePermission } from "#/auth/permission.middleware.ts";
import { userRoleUpdateSchema } from "#/auth/schemas.ts";
import { listModerationActions, logModeration } from "./moderation.server.ts";
import { moderationLogRequestSchema, moderationReasonSchema } from "./schemas.ts";

/**
 * The part of the admin plugin this module calls.
 *
 * `getAuth` is typed against the generic options so that type checking stays
 * cheap, which leaves the plugin's endpoints off the type. They are named
 * here instead, which is narrow enough to notice when the plugin changes.
 */
interface AdminApi {
  setRole(input: { body: { userId: string; role: string }; headers: Headers }): Promise<unknown>;
  banUser(input: {
    body: { userId: string; banReason?: string; banExpiresIn?: number };
    headers: Headers;
  }): Promise<unknown>;
  unbanUser(input: { body: { userId: string }; headers: Headers }): Promise<unknown>;
}

function adminApi(): AdminApi {
  return getAuth().api as unknown as AdminApi;
}

/** Read the log, newest first, narrowed to a subject or to an actor. */
export const listModerationActionsFn = createServerFn({ method: "GET" })
  .middleware([requirePermission({ user: ["list"] })])
  .validator(moderationLogRequestSchema)
  .handler(async ({ data }) => ({ actions: await listModerationActions(data) }));

/**
 * Give a user a role.
 *
 * The write goes through the plugin so that the plugin stays the one thing
 * that knows how the column is shaped. The record is written after it: these
 * two are not one transaction, because the write is not ours to batch.
 */
export const setUserRoleFn = createServerFn({ method: "POST" })
  .middleware([requirePermission({ user: ["set-role"] })])
  .validator(userRoleUpdateSchema)
  .handler(async ({ data, context }) => {
    await adminApi().setRole({
      body: { userId: data.userId, role: data.role },
      headers: getRequest().headers,
    });
    await logModeration({
      actor: context.actor,
      action: "user.set-role",
      subjectType: "user",
      subjectId: data.userId,
      reason: data.reason,
      details: { role: data.role },
    });
    return { role: data.role };
  });

export const banUserRequestSchema = z.object({
  userId: z.string().min(1),
  /** What the user is told when they are turned away. */
  publicReason: z.string().trim().min(3).max(200),
  /** Why the moderator acted, for the record. */
  reason: moderationReasonSchema,
  /** Seconds the ban holds. Left out, it does not expire. */
  expiresInSeconds: z.number().int().positive().optional(),
});

export const banUserFn = createServerFn({ method: "POST" })
  .middleware([requirePermission({ user: ["ban"] })])
  .validator(banUserRequestSchema)
  .handler(async ({ data, context }) => {
    await adminApi().banUser({
      body: {
        userId: data.userId,
        banReason: data.publicReason,
        banExpiresIn: data.expiresInSeconds,
      },
      headers: getRequest().headers,
    });
    await logModeration({
      actor: context.actor,
      action: "user.ban",
      subjectType: "user",
      subjectId: data.userId,
      reason: data.reason,
      details: { publicReason: data.publicReason, expiresInSeconds: data.expiresInSeconds ?? null },
    });
    return { banned: true };
  });

export const unbanUserFn = createServerFn({ method: "POST" })
  .middleware([requirePermission({ user: ["ban"] })])
  .validator(z.object({ userId: z.string().min(1), reason: moderationReasonSchema }))
  .handler(async ({ data, context }) => {
    await adminApi().unbanUser({
      body: { userId: data.userId },
      headers: getRequest().headers,
    });
    await logModeration({
      actor: context.actor,
      action: "user.unban",
      subjectType: "user",
      subjectId: data.userId,
      reason: data.reason,
    });
    return { banned: false };
  });

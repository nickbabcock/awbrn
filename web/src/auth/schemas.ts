import { z } from "zod";
import { ROLE_NAMES } from "./access.ts";

export const authSignInSchema = z.object({
  email: z.string().email(),
  password: z.string().min(1),
});

export const authSignUpSchema = authSignInSchema.extend({
  name: z.string().trim().min(1),
});

export type AuthSignInInput = z.infer<typeof authSignInSchema>;
export type AuthSignUpInput = z.infer<typeof authSignUpSchema>;

/**
 * A role a user can be given.
 *
 * The column stores a comma separated list, so this is one entry of that
 * list and not the whole column. Nothing today gives out more than one.
 */
export const userRoleSchema = z.enum(ROLE_NAMES);

export type UserRole = z.infer<typeof userRoleSchema>;

/** Give a user a role, or put them back on the default one. */
export const userRoleUpdateSchema = z.object({
  userId: z.string().min(1),
  role: userRoleSchema,
  reason: z.string().trim().min(3).max(500),
});

export type UserRoleUpdate = z.infer<typeof userRoleUpdateSchema>;

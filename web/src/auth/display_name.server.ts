import { and, eq, ne } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { env } from "cloudflare:workers";
import * as schema from "#/db/global.ts";
import { displayNameKey, displayNameSchema } from "./display_name.ts";

export type DisplayNameClaim =
  | { ok: true; name: string; nameKey: string }
  | { ok: false; error: { message: string } };

/**
 * Check a name for `userId`, or for a new account when it is null.
 *
 * The unique index on `nameKey` still decides a race between two claims.
 * This check is what turns the common case into a message a player can read.
 */
export async function claimDisplayName(
  raw: string,
  userId: string | null,
): Promise<DisplayNameClaim> {
  const parsed = displayNameSchema.safeParse(raw);
  if (!parsed.success) {
    return { ok: false, error: { message: parsed.error.issues[0]?.message ?? "Invalid name" } };
  }

  const name = parsed.data;
  const nameKey = displayNameKey(name);
  const db = drizzle(env.DB, { schema });
  const taken = await db
    .select({ id: schema.user.id })
    .from(schema.user)
    .where(
      userId === null
        ? eq(schema.user.nameKey, nameKey)
        : and(eq(schema.user.nameKey, nameKey), ne(schema.user.id, userId)),
    )
    .get();

  if (taken) return { ok: false, error: { message: "That name is taken" } };
  return { ok: true, name, nameKey };
}

/**
 * One account for each role, so development can be somebody.
 *
 * Almost every rule in this application turns on who is asking, and half of
 * them turn on two people at once: a moderator grading a map somebody else
 * wrote is a different answer than the author asking about their own. A local
 * database with one account cannot reach those answers at all, so it starts
 * with one account per role instead.
 *
 * The accounts are made through the auth API rather than written into the
 * tables, because the password is hashed by whatever better-auth hashes with
 * and forging that by hand is a thing that breaks quietly on an upgrade. The
 * role is written afterwards, since signing up always gives the default one.
 *
 * Sign in as any of them at `/auth` with the password below. Two roles at
 * once is two browser profiles, or one window and one private window.
 */

import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import { eq } from "drizzle-orm";
import { user } from "#/db/global.ts";
import { getAuth } from "./auth.server.ts";
import type { RoleName } from "./access.ts";

/** The password every seeded account holds. Development only, and not a secret. */
export const DEV_ACCOUNT_PASSWORD = "awbrn-dev-password";

interface DevAccount {
  email: string;
  name: string;
  role: RoleName;
}

/**
 * The cast a local database starts with.
 *
 * Two players and not one, because ownership rules need somebody who is not
 * the person looking. `mapRankGrant` refusing an author is only reachable
 * when the author is a real row that somebody can sign in as.
 */
const DEV_ACCOUNTS: readonly DevAccount[] = [
  { email: "player@awbrn.test", name: "Andy", role: "user" },
  { email: "rival@awbrn.test", name: "Sami", role: "user" },
  { email: "moderator@awbrn.test", name: "Nell", role: "moderator" },
  { email: "admin@awbrn.test", name: "Hawke", role: "admin" },
];

const db = () => drizzle(env.DB, { schema: { user } });

let seeded: Promise<Map<string, string>> | null = null;

/**
 * Put the seed accounts in the database, once for each server that starts.
 *
 * The result maps each seeded email to the user id it holds, which is what
 * lets the map seed say who wrote which map. A failed seed is not remembered:
 * the next request tries again.
 */
export function seedDevAccounts(): Promise<Map<string, string>> {
  seeded ??= runSeed().catch((error: unknown) => {
    seeded = null;
    console.error("[dev-seed] could not seed the accounts", error);
    return new Map<string, string>();
  });
  return seeded;
}

async function runSeed(): Promise<Map<string, string>> {
  const ids = new Map<string, string>();

  for (const account of DEV_ACCOUNTS) {
    const id = (await findAccount(account.email)) ?? (await createAccount(account));
    if (id === null) continue;
    ids.set(account.email, id);
    // The role is written every time rather than only at creation, so an
    // account whose role was changed by hand comes back to what it says here.
    await db().update(user).set({ role: account.role }).where(eq(user.id, id));
  }

  if (ids.size > 0) {
    console.log(
      `[dev-seed] accounts ready, password "${DEV_ACCOUNT_PASSWORD}": ${DEV_ACCOUNTS.map(
        (account) => `${account.email} (${account.role})`,
      ).join(", ")}`,
    );
  }
  return ids;
}

async function findAccount(email: string): Promise<string | null> {
  const row = await db().select({ id: user.id }).from(user).where(eq(user.email, email)).get();
  return row?.id ?? null;
}

/**
 * Sign one account up, and report the id it was given.
 *
 * A sign-up that fails is logged and skipped rather than thrown, because one
 * account the seed cannot make should not stop the server answering.
 */
async function createAccount(account: DevAccount): Promise<string | null> {
  try {
    await getAuth().api.signUpEmail({
      body: {
        email: account.email,
        name: account.name,
        password: DEV_ACCOUNT_PASSWORD,
      },
    });
  } catch (error: unknown) {
    console.error(`[dev-seed] could not create ${account.email}`, error);
    return null;
  }
  return findAccount(account.email);
}

import { betterAuth } from "better-auth/minimal";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import type { Auth, BetterAuthOptions } from "better-auth/types";
import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import * as schema from "../db";

let _auth: Auth<BetterAuthOptions> | undefined;

export function getAuth(): Auth<BetterAuthOptions> {
  return (_auth ??= betterAuth({
    baseURL: env.AUTH_URL,
    secret: env.AUTH_SECRET,
    database: drizzleAdapter(drizzle(env.DB, { schema }), {
      provider: "sqlite",
    }),
    emailAndPassword: { enabled: true },
    session: {
      cookieCache: {
        enabled: true,
        maxAge: 60 * 5,
      },
    },
    advanced: {
      cookiePrefix: "awbrn",
    },
  }) as Auth<BetterAuthOptions>);
}

export type Session = Auth<BetterAuthOptions>["$Infer"]["Session"];

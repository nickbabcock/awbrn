import { betterAuth } from "better-auth/minimal";
import { admin } from "better-auth/plugins/admin";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { tanstackStartCookies } from "better-auth/tanstack-start";
import type { Auth, BetterAuthOptions } from "better-auth/types";
import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import * as schema from "#/db/global.ts";
import { ac, ADMIN_ROLES, DEFAULT_ROLE, ROLES } from "./access.ts";
import type { Session } from "./session";

let _auth: Auth<BetterAuthOptions> | undefined;

const authOptions: BetterAuthOptions = {
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
  plugins: [
    admin({
      ac,
      roles: ROLES,
      defaultRole: DEFAULT_ROLE,
      adminRoles: ADMIN_ROLES,
    }),
    tanstackStartCookies(),
  ],
};

export function getAuth(): Auth<BetterAuthOptions> {
  return (_auth ??= betterAuth(authOptions));
}

export async function getRequestSession(request: Request): Promise<Session | null> {
  return getAuth().api.getSession({ headers: request.headers });
}

export async function requireRequestSession(request: Request): Promise<Session> {
  const session = await getRequestSession(request);

  if (!session) {
    throw new Response("Unauthorized", { status: 401 });
  }

  return session;
}

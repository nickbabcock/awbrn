import { betterAuth } from "better-auth/minimal";
import { admin } from "better-auth/plugins/admin";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { tanstackStartCookies } from "better-auth/tanstack-start";
import { APIError } from "better-auth/api";
import type { Auth, BetterAuthOptions } from "better-auth/types";
import { env } from "cloudflare:workers";
import { eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import * as schema from "#/db/global.ts";
import { ac, ADMIN_ROLES, DEFAULT_ROLE, ROLES } from "./access.ts";
import { DISPLAY_NAME_COOLDOWN_MS } from "./display_name.ts";
import { claimDisplayName } from "./display_name.server.ts";
import type { Session } from "./session";

let _auth: Auth<BetterAuthOptions> | undefined;

const authOptions: BetterAuthOptions = {
  baseURL: env.AUTH_URL,
  secret: env.AUTH_SECRET,
  database: drizzleAdapter(drizzle(env.DB, { schema }), {
    provider: "sqlite",
  }),
  emailAndPassword: { enabled: true },
  user: {
    additionalFields: {
      nameKey: { type: "string", required: false, input: false, returned: false },
      nameChangedAt: { type: "date", required: false, input: false },
    },
  },
  /*
   * Every write of a name passes here: sign-up, the player renaming themselves
   * through `/update-user`, and an admin through `/admin/update-user`. The
   * moderator rename writes D1 itself and calls `claimDisplayName` directly.
   */
  databaseHooks: {
    user: {
      create: {
        async before(user) {
          const claim = await claimDisplayName(user.name, null);
          if (!claim.ok) throw new APIError("BAD_REQUEST", { message: claim.error.message });
          return { data: { ...user, name: claim.name, nameKey: claim.nameKey } };
        },
      },
      update: {
        async before(data, ctx) {
          if (data.name === undefined) return;

          const isSelf = ctx?.path === "/update-user";
          const userId: unknown = isSelf
            ? ctx.context.session?.user.id
            : ctx?.path === "/admin/update-user"
              ? ctx.body?.userId
              : undefined;
          if (typeof userId !== "string") {
            throw new APIError("BAD_REQUEST", { message: "Names cannot be changed here" });
          }

          const claim = await claimDisplayName(data.name, userId);
          if (!claim.ok) throw new APIError("BAD_REQUEST", { message: claim.error.message });
          const named = { ...data, name: claim.name, nameKey: claim.nameKey };
          if (!isSelf) return { data: named };

          const current = await drizzle(env.DB, { schema })
            .select({ name: schema.user.name, nameChangedAt: schema.user.nameChangedAt })
            .from(schema.user)
            .where(eq(schema.user.id, userId))
            .get();
          if (current?.name === claim.name) return { data: named };
          const lastChange = current?.nameChangedAt?.getTime();
          if (lastChange !== undefined && Date.now() - lastChange < DISPLAY_NAME_COOLDOWN_MS) {
            throw new APIError("BAD_REQUEST", {
              message: "You can rename once every 30 days",
            });
          }
          return { data: { ...named, nameChangedAt: new Date() } };
        },
      },
    },
  },
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

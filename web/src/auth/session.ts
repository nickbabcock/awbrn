import type { Auth, BetterAuthOptions } from "better-auth/types";

type BaseSession = Auth<BetterAuthOptions>["$Infer"]["Session"];

/**
 * The session, with the fields the admin plugin puts on the user.
 *
 * `getAuth` is typed against the generic options so that type checking stays
 * cheap, which means the plugin's fields are named here rather than read off
 * the configuration. They are optional because a session that predates the
 * plugin carries none of them.
 */
export interface Session extends BaseSession {
  user: BaseSession["user"] & {
    role?: string | null;
    banned?: boolean | null;
    banReason?: string | null;
    banExpires?: Date | string | null;
  };
}

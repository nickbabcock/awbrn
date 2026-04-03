import type { Auth, BetterAuthOptions } from "better-auth/types";

export type Session = Auth<BetterAuthOptions>["$Infer"]["Session"];

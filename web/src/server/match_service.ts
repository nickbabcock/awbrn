import { env } from "cloudflare:workers";

import type { MatchDurableObject } from "./match_durable_object";

export type MatchStub = DurableObjectStub<MatchDurableObject>;

export function createMatchStub(): MatchStub {
  const durableObjectId = env.MATCHES.newUniqueId();
  return env.MATCHES.get(durableObjectId);
}

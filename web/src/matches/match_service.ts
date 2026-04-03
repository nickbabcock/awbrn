import { env } from "cloudflare:workers";

import type { MatchDurableObject } from "./match_durable_object";

export type MatchStub = DurableObjectStub<MatchDurableObject>;

export function matchDurableObjectName(matchId: string): string {
  return `match:${matchId}`;
}

export function getMatchStub(matchId: string): MatchStub {
  const durableObjectId = env.MATCHES.idFromName(matchDurableObjectName(matchId));
  return env.MATCHES.get(durableObjectId);
}

import { env } from "cloudflare:workers";

import type { PlayerDurableObject } from "./player_durable_object.ts";

export type PlayerStub = DurableObjectStub<PlayerDurableObject>;

export function playerDurableObjectName(userId: string): string {
  return `player:${userId}`;
}

/**
 * The object that speaks for one player.
 *
 * The name is derived from the player's own id, so there is no table to keep
 * and no lookup to make: any worker holding a checked session can reach the
 * right object straight away.
 */
export function getPlayerStub(userId: string): PlayerStub {
  return env.PLAYERS.getByName(playerDurableObjectName(userId));
}

/** The same object, reached from another durable object's bindings. */
export function getPlayerStubFrom(
  binding: DurableObjectNamespace<PlayerDurableObject>,
  userId: string,
): PlayerStub {
  return binding.getByName(playerDurableObjectName(userId));
}

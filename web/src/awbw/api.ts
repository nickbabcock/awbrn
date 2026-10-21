import { awbwGameSnapshotSchema, type AwbwGameSnapshot } from "./schemas";

const usernameCache = new Map<number, Promise<string | null>>();

interface UsernameResponse {
  userId: number;
  username: string | null;
}

export function resolveAwbwUsername(userId: number): Promise<string | null> {
  const cached = usernameCache.get(userId);
  if (cached) {
    return cached;
  }

  const request = fetch(`/api/awbw/user/${userId}`)
    .then(async (response) => {
      if (!response.ok) {
        return null;
      }

      const payload = (await response.json()) as UsernameResponse;
      if (payload.userId !== userId) {
        return null;
      }

      return payload.username;
    })
    .catch(() => null);

  usernameCache.set(userId, request);
  return request;
}

/**
 * The armies, map and turn of an AWBW game, read from its game page.
 *
 * Throws an error with a message for the player when the game cannot be read.
 */
export async function fetchAwbwGame(gameId: number): Promise<AwbwGameSnapshot> {
  const response = await fetch(`/api/awbw/game/${gameId}`);
  if (response.status === 404) {
    throw new Error(`AWBW has no game ${gameId}.`);
  }
  if (response.status === 429) {
    throw new Error("Too many AWBW requests. Try again in a moment.");
  }
  if (!response.ok) {
    throw new Error("AWBW did not answer. Try again later.");
  }

  const parsed = awbwGameSnapshotSchema.safeParse(await response.json());
  if (!parsed.success || parsed.data.gameId !== gameId) {
    throw new Error("The AWBW game page could not be read.");
  }
  return parsed.data;
}

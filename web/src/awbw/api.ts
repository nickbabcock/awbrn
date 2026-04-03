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

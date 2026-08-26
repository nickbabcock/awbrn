import { env } from "cloudflare:workers";

const RETRY_AFTER_SECONDS = 60;

/** Throw a Response when the limit rejects; callers must catch and return it explicitly. */
export async function requireRateLimit(
  limiter: RateLimit,
  key: string,
  retryAfterSeconds = RETRY_AFTER_SECONDS,
): Promise<void> {
  const { success } = await limiter.limit({ key });
  if (success) return;

  throw new Response("Too Many Requests", {
    status: 429,
    headers: { "Retry-After": String(retryAfterSeconds) },
  });
}

export function requestActorKey(request: Request): string {
  const address = request.headers.get("CF-Connecting-IP");
  return address ? `ip:${address}` : "ip:unknown";
}

export function rateLimitBindings() {
  return env;
}

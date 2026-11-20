import { redirect } from "@tanstack/react-router";
import type { QueryClient } from "@tanstack/react-query";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";

/** Sends a signed-out visitor through sign-in and back to `href`. */
export async function requireSignIn(queryClient: QueryClient, href: string): Promise<void> {
  const session = await queryClient.ensureQueryData(sessionQueryOptions());
  if (!session) throw redirect({ search: { redirect: href }, to: "/auth" });
}

/** A path on this site, never another origin. */
export function signInReturn(value: unknown): string | undefined {
  if (typeof value !== "string" || !value.startsWith("/")) return undefined;
  // Resolved the way a browser resolves it, which reads `\` as `/` and drops
  // tabs and newlines.
  let url: URL;
  try {
    url = new URL(value, SAME_ORIGIN);
  } catch {
    return undefined;
  }
  const path = url.pathname + url.search + url.hash;
  // `/.//evil.com` resolves to a path that a browser reads as another host.
  return url.origin === SAME_ORIGIN && !path.startsWith("//") ? path : undefined;
}

const SAME_ORIGIN = "http://same-origin.invalid";

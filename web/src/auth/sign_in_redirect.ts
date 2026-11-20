import { redirect } from "@tanstack/react-router";
import type { QueryClient } from "@tanstack/react-query";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";

/** Sends a signed-out visitor through sign-in and back to `href`. */
export async function requireSignIn(queryClient: QueryClient, href: string): Promise<void> {
  const session = await queryClient.ensureQueryData(sessionQueryOptions());
  if (!session) throw redirect({ search: { redirect: href }, to: "/auth" });
}

/** A path on this site, never another origin. Browsers read `\` as `/`. */
export function signInReturn(value: unknown): string | undefined {
  return typeof value === "string" &&
    value.startsWith("/") &&
    !value.startsWith("//") &&
    !value.includes("\\")
    ? value
    : undefined;
}

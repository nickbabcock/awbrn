import { useSuspenseQuery } from "@tanstack/react-query";
import { sessionQueryOptions } from "./auth.queries";
import type { Session } from "./session";

export function useAppSession(): Session | null {
  return useSuspenseQuery(sessionQueryOptions()).data;
}

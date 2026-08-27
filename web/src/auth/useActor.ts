import { useMemo } from "react";
import { useAppSession } from "./useAppSession.ts";
import { viewerActor, type Actor } from "./actor.ts";

/**
 * The actor a screen draws with.
 *
 * The role comes from the cached session, so this decides which buttons to
 * draw and never whether a write lands. A screen that shows a button the
 * server then refuses is a stale cache and not a hole: the server resolves
 * its own actor from the database.
 */
export function useActor(): Actor | null {
  const session = useAppSession();
  return useMemo(() => viewerActor(session), [session]);
}

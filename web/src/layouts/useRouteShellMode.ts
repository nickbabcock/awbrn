import { useQuery } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";

/**
 * The shell mode a route implies, so a match and the task screens get their
 * frame in the first paint without each screen having to remember it. A
 * screen can still state its own mode with `<ShellMode>`; the replay viewer
 * does so once an archive is loaded (its upload page is a Replays section page
 * with the tab bar).
 *
 * A match's address holds four screens, so its phase decides: an active or a
 * finished match is pushed (the board), a lobby or a ranked pairing is a task.
 * The route loader has the match in the query cache before this renders.
 */
export function useRouteShellMode(): "pushed" | "task" | null {
  const location = useRouterState({
    select: (state) => ({ pathname: state.location.pathname, search: state.location.search }),
  });
  const pathname = location.pathname.replace(/\/$/, "") || "/";
  const routeMatchId = /^\/matches\/([^/]+)$/.exec(pathname)?.[1];
  const matchId = routeMatchId && routeMatchId !== "new" ? routeMatchId : null;
  const search = location.search as { join?: unknown };
  const joinSlug = typeof search.join === "string" ? search.join : null;
  // Read from the cache and follow it (a lobby that starts becomes a board),
  // but never fetch from here: the route owns loading the match.
  const { data: match } = useQuery({
    ...matchDetailQueryOptions(matchId ?? "", joinSlug),
    enabled: false,
  });

  if (pathname === "/matches/new") return "task";
  if (!matchId || !match) return null;
  return match.phase === "active" || match.phase === "completed" ? "pushed" : "task";
}

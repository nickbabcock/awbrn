import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect } from "react";
import { matchKeys } from "#/matches/matches.keys.ts";
import { useRatingChanges } from "./rating_changes.ts";
import { usePlayerSocket, type PlayerSocketStatus } from "./player_websocket.ts";
import { faviconDataUrl, tabTitle } from "./tab_badge.ts";
import type { PlayerSocketMessage } from "./player_protocol.ts";

/**
 * Keep this tab's counts current from the player's own socket.
 *
 * The counts themselves are still read from the database, because the socket
 * says only that something moved and the database is what knows how much is
 * waiting. What the socket replaces is the asking: a tab used to have to guess
 * how often whose turn it is might have changed.
 */
export function usePlayerNotifications(enabled: boolean): PlayerSocketStatus {
  const queryClient = useQueryClient();

  const onMessage = useCallback(
    (message: PlayerSocketMessage) => {
      switch (message.type) {
        case "turnStarted":
        case "turnEnded": {
          // The nav count and the list it sends a player to are refreshed
          // together, so the two can never disagree about the same match.
          void queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() });
          void queryClient.invalidateQueries({ queryKey: matchKeys.mine() });
          return;
        }
        case "ratingChanged": {
          // A report of this match may be open and waiting for the number.
          // The history list holds the same figure, so it is re-read too.
          useRatingChanges.getState().record(message);
          void queryClient.invalidateQueries({ queryKey: matchKeys.completed() });
          return;
        }
        default: {
          return;
        }
      }
    },
    [queryClient],
  );

  return usePlayerSocket(enabled, onMessage);
}

/**
 * Put everything waiting on the player into the tab itself.
 *
 * A player who is reading something else has the tab strip and nothing more,
 * so the count goes in the title and on the icon. `resetKey` changes when the
 * page does, because the router writes its own title on navigation and the
 * count has to be put back in front of it.
 */
export function useTabBadge(count: number, resetKey: string): void {
  useEffect(() => {
    if (typeof document === "undefined") return;

    const base = document.title.replace(/^\(\d+\+?\)\s+/, "");
    document.title = tabTitle(base, count);

    let icon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
    if (icon === null) {
      icon = document.createElement("link");
      icon.rel = "icon";
      document.head.appendChild(icon);
    }
    icon.type = "image/svg+xml";
    icon.href = faviconDataUrl(count);
  }, [count, resetKey]);
}

/**
 * Whether the browser is holding a push subscription for this player.
 *
 * It is read from the browser rather than from the site, because the browser
 * is where a subscription is actually revoked and a site that only asked
 * itself would report one the player had already turned off.
 */
export function usePushSubscriptionState(enabled: boolean) {
  return useQuery({
    queryKey: ["push", "subscription"],
    enabled,
    queryFn: async () => {
      const { currentSubscription, pushPermission } = await import("./push_subscription.ts");
      return {
        permission: pushPermission(),
        endpoint: (await currentSubscription())?.endpoint ?? null,
      };
    },
    staleTime: 0,
  });
}

import handler, { createServerEntry } from "@tanstack/react-start/server-entry";
import { MatchDurableObject } from "#/matches/match_durable_object.ts";
import { AwbwGatewayDurableObject } from "#/awbw/awbw_gateway.ts";
import { MatchmakerDurableObject } from "#/matchmaking/matchmaker_durable_object.ts";
import { PlayerDurableObject } from "#/players/player_durable_object.ts";
import { getMatchStub } from "#/matches/match_service.ts";
import { getPlayerStub } from "#/players/player_service.ts";
import { getRequestSession } from "#/auth/auth.server.ts";

export {
  AwbwGatewayDurableObject,
  MatchDurableObject,
  MatchmakerDurableObject,
  PlayerDurableObject,
};

const crossOriginIsolationHeaders = {
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Cross-Origin-Opener-Policy": "same-origin",
} as const;

const MATCH_WEBSOCKET_PATTERN = new URLPattern({
  pathname: "/api/matches/:matchId/ws",
});

/**
 * The one socket a signed-in player holds wherever they are on the site.
 *
 * There is no id in the path: the object is named after the session this
 * worker has just checked, so a player can only ever reach their own.
 */
const PLAYER_WEBSOCKET_PATTERN = new URLPattern({
  pathname: "/api/player/ws",
});

export default createServerEntry({
  async fetch(request) {
    // A local database starts with no maps and nobody to be, so development
    // fills the catalog and the cast before it answers. The branch is dropped
    // from a production build.
    if (import.meta.env.DEV) {
      const { seedDevAccounts } = await import("#/auth/dev_seed.server.ts");
      const { attributeDevMaps, seedDevMaps } = await import("#/maps/dev_seed.server.ts");
      const { seedDevRankedSeeks } = await import("#/matchmaking/dev_seed.server.ts");
      const [accounts] = await Promise.all([seedDevAccounts(), seedDevMaps()]);
      await attributeDevMaps(accounts);
      await seedDevRankedSeeks(accounts);
    }

    if (
      PLAYER_WEBSOCKET_PATTERN.test(request.url) &&
      request.headers.get("Upgrade") === "websocket"
    ) {
      const session = await getRequestSession(request);
      if (!session) {
        return new Response("Unauthorized", {
          status: 401,
          headers: crossOriginIsolationHeaders,
        });
      }
      return getPlayerStub(session.user.id).fetch(request);
    }

    const websocketMatch = MATCH_WEBSOCKET_PATTERN.exec(request.url);
    if (websocketMatch && request.headers.get("Upgrade") === "websocket") {
      const { matchId } = websocketMatch.pathname.groups;
      if (matchId) {
        return getMatchStub(matchId).fetch(request);
      }
      return new Response("Invalid match ID", {
        status: 400,
        headers: crossOriginIsolationHeaders,
      });
    }

    return handler.fetch(request);
  },
});

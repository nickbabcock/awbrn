import handler, { createServerEntry } from "@tanstack/react-start/server-entry";
import { MatchDurableObject } from "#/matches/match_durable_object.ts";
import { AwbwGatewayDurableObject } from "#/awbw/awbw_gateway.ts";
import { getMatchStub } from "#/matches/match_service.ts";

export { AwbwGatewayDurableObject, MatchDurableObject };

const crossOriginIsolationHeaders = {
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Cross-Origin-Opener-Policy": "same-origin",
} as const;

const MATCH_WEBSOCKET_PATTERN = new URLPattern({
  pathname: "/api/matches/:matchId/ws",
});

export default createServerEntry({
  async fetch(request) {
    // A local database starts with no maps, so development fills the catalog
    // before it answers. The branch is dropped from a production build.
    if (import.meta.env.DEV) {
      const { seedDevMaps } = await import("#/maps/dev_seed.server.ts");
      await seedDevMaps();
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

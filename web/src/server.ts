import handler, { createServerEntry } from "@tanstack/react-start/server-entry";
import { MatchDurableObject } from "#/matches/match_durable_object.ts";
import { getMatchStub } from "#/matches/match_service.ts";

export { MatchDurableObject };

const crossOriginIsolationHeaders = {
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Cross-Origin-Opener-Policy": "same-origin",
} as const;

const MATCH_WEBSOCKET_PATTERN = new URLPattern({
  pathname: "/api/matches/:matchId/ws",
});

export default createServerEntry({
  fetch(request) {
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

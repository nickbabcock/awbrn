import handler, { createServerEntry } from "@tanstack/react-start/server-entry";
import { MatchDurableObject } from "./matches/match_durable_object";

export { MatchDurableObject };

const MATCH_WEBSOCKET_PATTERN = new URLPattern({
  pathname: "/api/matches/:matchId/ws",
});

export default createServerEntry({
  fetch(request) {
    const websocketMatch = MATCH_WEBSOCKET_PATTERN.exec(request.url);
    if (websocketMatch && request.headers.get("Upgrade") === "websocket") {
      const { matchId } = websocketMatch.pathname.groups;
      // TODO: forward websocket upgrades to the match durable object via:
      // const stub = env.MATCHES.get(env.MATCHES.idFromString(matchId));
      // return stub.fetch(request);
      // This websocket path is reserved for future match traffic once the runtime protocol exists.
      return new Response(`Match websocket forwarding is not implemented for ${matchId}`, {
        status: 501,
      });
    }

    return handler.fetch(request);
  },
});

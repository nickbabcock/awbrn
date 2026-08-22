import { createFileRoute } from "@tanstack/react-router";
import { env } from "cloudflare:workers";
import { matchIdSchema } from "#/matches/match_id.ts";
import { getMatchReplayResponse } from "#/matches/replay_archive.ts";

export const Route = createFileRoute("/replays/{$matchId}.json")({
  params: {
    parse: ({ matchId }) => ({ matchId: matchIdSchema.parse(matchId) }),
    stringify: ({ matchId }) => ({ matchId }),
  },
  server: {
    handlers: {
      GET: ({ params }) => getMatchReplayResponse(env.CONTENT, params.matchId),
    },
  },
});

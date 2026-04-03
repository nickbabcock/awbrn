import { createFileRoute } from "@tanstack/react-router";
import { getRequestSession } from "../../../server/auth";
import { parseJsonBody, responseFromResult } from "../../../server/match_protocol";
import {
  getMatchSnapshot,
  mutateMatch,
  validateMatchMutationRequest,
} from "../../../server/match_store";

export const Route = createFileRoute("/api/matches/$matchId")({
  server: {
    handlers: {
      GET: async ({ request, params }) => {
        const session = await getRequestSession(request);
        const joinSlug = new URL(request.url).searchParams.get("join");
        const result = await getMatchSnapshot(params.matchId, session?.user.id ?? null, joinSlug);
        return responseFromResult(result);
      },
      POST: async ({ request, params }) => {
        const session = await getRequestSession(request);
        if (!session) {
          return Response.json(
            {
              error: {
                code: "unauthorized",
                message: "you must be signed in to update a lobby",
                httpStatus: 401,
              },
            },
            { status: 401 },
          );
        }

        const bodyResult = await parseJsonBody(request);
        if (!bodyResult.ok) {
          return responseFromResult(bodyResult);
        }

        const action = validateMatchMutationRequest(bodyResult.value);
        if (!action.ok) {
          return responseFromResult(action);
        }

        const result = await mutateMatch(params.matchId, session.user, action.value);
        return responseFromResult(result);
      },
    },
  },
});

import { createFileRoute } from "@tanstack/react-router";

import { getRequestSession } from "../../server/auth";
import { parseJsonBody, responseFromResult } from "../../server/match_protocol";
import { createMatch, validateMatchCreateRequest } from "../../server/match_store";

export const Route = createFileRoute("/api/matches")({
  server: {
    handlers: {
      POST: async ({ request }) => {
        const session = await getRequestSession(request);
        if (!session) {
          return Response.json(
            {
              error: {
                code: "unauthorized",
                message: "you must be signed in to create a match",
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

        const createRequest = validateMatchCreateRequest(bodyResult.value);
        if (!createRequest.ok) {
          return responseFromResult(createRequest);
        }

        const result = await createMatch(createRequest.value, {
          id: session.user.id,
          name: session.user.name,
        });
        return responseFromResult(result, 201);
      },
    },
  },
});

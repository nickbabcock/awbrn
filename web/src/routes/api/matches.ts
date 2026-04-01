import { createFileRoute } from "@tanstack/react-router";

import { createMatchStub } from "../../server/match_service";
import { parseJsonBody, responseFromResult } from "../../server/match_protocol";

export const Route = createFileRoute("/api/matches")({
  server: {
    handlers: {
      POST: async ({ request }) => {
        const bodyResult = await parseJsonBody(request);
        if (!bodyResult.ok) {
          return responseFromResult(bodyResult);
        }

        const stub = createMatchStub();
        const result = await stub.initializeMatch(bodyResult.value);
        return responseFromResult(result, 201);
      },
    },
  },
});

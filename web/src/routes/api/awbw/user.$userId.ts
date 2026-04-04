import { createFileRoute } from "@tanstack/react-router";
import { fetchAwbwUsernameResponse } from "#/awbw/awbw.server.ts";
import { parsePositiveIntegerParam } from "#/awbw/parsers.ts";

export const Route = createFileRoute("/api/awbw/user/$userId")({
  server: {
    handlers: {
      GET: async ({ params, request }) => {
        const userId = parsePositiveIntegerParam(params.userId);
        if (userId === null) {
          return Response.json({ error: "Invalid userId" }, { status: 400 });
        }

        return fetchAwbwUsernameResponse(request, userId);
      },
    },
  },
});

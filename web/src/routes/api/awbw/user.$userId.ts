import { createFileRoute } from "@tanstack/react-router";
import { fetchAwbwUsername } from "#/awbw/awbw.server.ts";
import { parsePositiveIntegerParam } from "#/awbw/parsers.ts";

export const Route = createFileRoute("/api/awbw/user/$userId")({
  server: {
    handlers: {
      GET: async ({ params }) => {
        const userId = parsePositiveIntegerParam(params.userId);
        if (userId === null) {
          return Response.json({ error: "Invalid userId" }, { status: 400 });
        }

        return fetchAwbwUsername(userId);
      },
    },
  },
});

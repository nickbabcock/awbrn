import { createFileRoute } from "@tanstack/react-router";
import { fetchAwbwUsername, parsePositiveIntegerParam } from "../../../utils/awbw";

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

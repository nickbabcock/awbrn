import { createFileRoute } from "@tanstack/react-router";
import { fetchAwbwGameResponse } from "#/awbw/awbw.server.ts";
import { parsePositiveIntegerParam } from "#/awbw/parsers.ts";

export const Route = createFileRoute("/api/awbw/game/$gameId")({
  server: {
    handlers: {
      GET: async ({ params, request }) => {
        const gameId = parsePositiveIntegerParam(params.gameId);
        if (gameId === null) {
          return Response.json({ error: "Invalid gameId" }, { status: 400 });
        }

        return fetchAwbwGameResponse(request, gameId);
      },
    },
  },
});

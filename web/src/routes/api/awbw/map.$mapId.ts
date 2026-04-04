import { createFileRoute } from "@tanstack/react-router";
import { fetchAwbwMapResponse } from "#/awbw/awbw.server.ts";
import { parsePositiveIntegerParam } from "#/awbw/parsers.ts";

export const Route = createFileRoute("/api/awbw/map/$mapId")({
  server: {
    handlers: {
      GET: async ({ params, request }) => {
        const mapId = parsePositiveIntegerParam(params.mapId.replace(/\.json$/i, ""));
        if (mapId === null) {
          return Response.json({ error: "Invalid mapId" }, { status: 400 });
        }

        return fetchAwbwMapResponse(request, mapId);
      },
    },
  },
});

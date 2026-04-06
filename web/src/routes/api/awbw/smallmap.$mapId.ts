import { createFileRoute } from "@tanstack/react-router";
import { fetchAwbwSmallMapResponse } from "#/awbw/awbw.server.ts";
import { parsePositiveIntegerParam } from "#/awbw/parsers.ts";

export const Route = createFileRoute("/api/awbw/smallmap/$mapId")({
  server: {
    handlers: {
      GET: async ({ params, request }) => {
        const mapId = parsePositiveIntegerParam(params.mapId.replace(/\.png$/i, ""));
        if (mapId === null) {
          return Response.json({ error: "Invalid mapId" }, { status: 400 });
        }

        return fetchAwbwSmallMapResponse(request, mapId);
      },
    },
  },
});

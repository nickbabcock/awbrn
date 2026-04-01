import { createFileRoute } from "@tanstack/react-router";
import { fetchAwbwMap, parsePositiveIntegerParam } from "../../../utils/awbw";

export const Route = createFileRoute("/api/awbw/map/$mapId")({
  server: {
    handlers: {
      GET: async ({ params }) => {
        const mapId = parsePositiveIntegerParam(params.mapId);
        if (mapId === null) {
          return Response.json({ error: "Invalid mapId" }, { status: 400 });
        }

        return fetchAwbwMap(mapId);
      },
    },
  },
});

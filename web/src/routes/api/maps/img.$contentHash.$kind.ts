import { createFileRoute } from "@tanstack/react-router";
import { readMapScreenshot } from "#/maps/maps.server.ts";
import { MAP_SCREENSHOT_KINDS, type MapScreenshotKind } from "#/maps/map_screenshot.ts";

const CONTENT_HASH_PATTERN = /^[0-9a-f]{64}$/;

function parseScreenshotKind(value: string): MapScreenshotKind | null {
  const kind = value.replace(/\.png$/i, "");
  return (MAP_SCREENSHOT_KINDS as readonly string[]).includes(kind)
    ? (kind as MapScreenshotKind)
    : null;
}

export const Route = createFileRoute("/api/maps/img/$contentHash/$kind")({
  server: {
    handlers: {
      GET: async ({ params }) => {
        if (!CONTENT_HASH_PATTERN.test(params.contentHash)) {
          return Response.json({ error: "Invalid map content hash" }, { status: 400 });
        }

        const kind = parseScreenshotKind(params.kind);
        if (kind === null) {
          return Response.json({ error: "Invalid map picture" }, { status: 400 });
        }

        return readMapScreenshot(params.contentHash, kind);
      },
    },
  },
});

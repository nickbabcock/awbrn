import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import type { ViteDevServer } from "vite";

function decodeHtmlEntities(value: string): string {
  return value
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#039;", "'");
}

function extractAwbwUsername(html: string): string | null {
  const usernameIndex = html.indexOf("Username:");
  if (usernameIndex < 0) {
    return null;
  }

  const startMarker = html.indexOf("<i>", usernameIndex);
  if (startMarker < 0) {
    return null;
  }

  const start = startMarker + 3;
  const end = html.indexOf("</i>", start);
  if (end < 0) {
    return null;
  }

  return decodeHtmlEntities(html.slice(start, end).trim());
}

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    {
      name: "custom-map-asset-handler",
      configureServer(server: ViteDevServer) {
        server.middlewares.use(async (req, res, next) => {
          const mapRegex = /^\/assets\/maps\/(\d+)\.json$/;

          // Try different URL properties that might exist
          const url = req.originalUrl ?? req.url ?? "";
          const match = url.match(mapRegex);

          if (!match) {
            const usernameRegex = /^\/api\/awbw\/username(?:\?(.+))?$/;
            const usernameMatch = url.match(usernameRegex);
            if (!usernameMatch) {
              next();
              return;
            }

            const parsedUrl = new URL(url, "http://localhost");
            const userId = parsedUrl.searchParams.get("userId");
            if (!userId) {
              res.statusCode = 400;
              res.setHeader("Content-Type", "application/json");
              res.end(JSON.stringify({ error: "Missing userId query param" }));
              return;
            }

            const resp = await fetch(`https://awbw.amarriner.com/profile.php?users_id=${userId}`);
            if (!resp.ok) {
              server.config.logger.error(`Failed to fetch username for user ${userId}`);
              res.statusCode = 502;
              res.setHeader("Content-Type", "application/json");
              res.end(JSON.stringify({ userId: Number(userId), username: null }));
              return;
            }

            const html = await resp.text();
            const username = extractAwbwUsername(html);
            res.statusCode = 200;
            res.setHeader("Content-Type", "application/json");
            res.end(JSON.stringify({ userId: Number(userId), username }));
            return;
          }

          const mapId = match[1];
          server.config.logger.info(`Intercepting request for map ${mapId}.json`);

          const resp = await fetch(
            `https://awbw.amarriner.com/api/map/map_info.php?maps_id=${mapId}`,
          );
          if (!resp.ok) {
            server.config.logger.error(`Failed to fetch map ${mapId}.json`);
            res.statusCode = 404;
            res.end("Not Found");
            return;
          }

          res.setHeaders(resp.headers);
          res.end(new Uint8Array(await resp.arrayBuffer()));
          return;
        });
      },
    },
  ],
  server: {
    fs: {
      allow: [".."],
    },
  },
});

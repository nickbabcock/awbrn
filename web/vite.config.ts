import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import type { ViteDevServer } from "vite";

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
            next();
            return;
          }

          const mapId = match[1];
          server.config.logger.info(
            `Intercepting request for map ${mapId}.json`,
          );

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

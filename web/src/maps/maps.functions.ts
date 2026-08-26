import { createServerFn } from "@tanstack/react-start";
import { sessionMiddleware } from "#/auth/session.middleware.ts";
import { awbwMapImportRequestSchema, mapCatalogRequestSchema, mapRefSchema } from "./schemas.ts";
import { importAwbwMapToCatalog, listCatalogMaps, loadMapRevision } from "./maps.server.ts";

export const getMapRevisionFn = createServerFn({ method: "GET" })
  .validator(mapRefSchema)
  .handler(async ({ data }) => loadMapRevision(data));

export const listMapsFn = createServerFn({ method: "GET" })
  .validator(mapCatalogRequestSchema)
  .handler(async ({ data }) => listCatalogMaps(data));

/**
 * Put an AWBW map in the catalog.
 *
 * Importing is held behind a session because it spends a fetch to AWBW and a
 * render for every map it adds.
 */
export const importAwbwMapFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(awbwMapImportRequestSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Error("you must be signed in to import a map");
    return importAwbwMapToCatalog(data.sourceMapId);
  });

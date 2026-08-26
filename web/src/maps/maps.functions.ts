import { createServerFn } from "@tanstack/react-start";
import { sessionMiddleware } from "#/auth/session.middleware.ts";
import {
  awbwMapImportRequestSchema,
  mapCatalogRequestSchema,
  mapRankUpdateSchema,
  mapRefSchema,
  mapTagsUpdateSchema,
} from "./schemas.ts";
import {
  importAwbwMapToCatalog,
  listCatalogMaps,
  loadMapRevision,
  setMapRevisionRank,
  setMapTags,
} from "./maps.server.ts";
import { rateLimitBindings, requireRateLimit } from "#/rate_limit.ts";

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
    await requireRateLimit(
      rateLimitBindings().IMPORT_MAP_RATE_LIMITER,
      `user:${context.session.user.id}`,
    );
    return importAwbwMapToCatalog(data.sourceMapId);
  });

/** Rank a map revision, or take away the rank it holds. */
export const setMapRankFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(mapRankUpdateSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Error("you must be signed in to rank a map");
    await setMapRevisionRank(data.map, data.rank);
    return { rank: data.rank };
  });

/** Replace every tag on a map. */
export const setMapTagsFn = createServerFn({ method: "POST" })
  .middleware([sessionMiddleware])
  .validator(mapTagsUpdateSchema)
  .handler(async ({ data, context }) => {
    if (!context.session) throw new Error("you must be signed in to tag a map");
    return { tags: await setMapTags(data.mapId, data.tags) };
  });

import { createServerFn } from "@tanstack/react-start";
import { actorMiddleware, requirePermission } from "#/auth/permission.middleware.ts";
import {
  awbwMapImportRequestSchema,
  mapCatalogRequestSchema,
  mapRankUpdateSchema,
  mapRefSchema,
  mapTagsUpdateSchema,
} from "./schemas.ts";
import {
  findCatalogEntry,
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

/**
 * One catalog entry, which is where a screen gets a map's pictures.
 *
 * A match names a map revision and nothing else, so a screen that wants to
 * show the board asks for the entry rather than drawing the document.
 */
export const getMapCatalogEntryFn = createServerFn({ method: "GET" })
  .validator(mapRefSchema)
  .handler(async ({ data }) => findCatalogEntry(data));

export const listMapsFn = createServerFn({ method: "GET" })
  .validator(mapCatalogRequestSchema)
  .handler(async ({ data }) => listCatalogMaps(data));

/**
 * Put an AWBW map in the catalog.
 *
 * Importing is held behind a permission because it spends a fetch to AWBW and
 * a render for every map it adds. Every signed-in player holds it; resolving
 * the actor is also what shuts a banned account out.
 */
export const importAwbwMapFn = createServerFn({ method: "POST" })
  .middleware([requirePermission({ map: ["import"] })])
  .validator(awbwMapImportRequestSchema)
  .handler(async ({ data, context }) => {
    await requireRateLimit(
      rateLimitBindings().IMPORT_MAP_RATE_LIMITER,
      `user:${context.actor.userId}`,
    );
    return importAwbwMapToCatalog(data.sourceMapId);
  });

/**
 * Rank a map revision, or take away the rank it holds.
 *
 * The middleware answers the role, which is the cheap half of the question.
 * The other half turns on the map: nobody ranks their own work, so
 * `setMapRevisionRank` loads the author and puts it to `mapRankGrant`.
 */
export const setMapRankFn = createServerFn({ method: "POST" })
  .middleware([requirePermission({ map: ["rank"] })])
  .validator(mapRankUpdateSchema)
  .handler(async ({ data, context }) => {
    await setMapRevisionRank(data.map, data.rank, {
      actor: context.actor,
      reason: data.reason,
    });
    return { rank: data.rank };
  });

/**
 * Replace every tag on a map.
 *
 * The author of a map may tag it and so may a moderator, which is a question
 * the middleware cannot answer because it turns on the map. `setMapTags`
 * loads the map and puts it to `mapTagGrant`.
 */
export const setMapTagsFn = createServerFn({ method: "POST" })
  .middleware([actorMiddleware])
  .validator(mapTagsUpdateSchema)
  .handler(async ({ data, context }) => {
    return {
      tags: await setMapTags({
        mapId: data.mapId,
        tags: data.tags,
        actor: context.actor,
        reason: data.reason,
      }),
    };
  });

import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import { and, eq } from "drizzle-orm";
import { fetchAwbwMapData } from "#/awbw/awbw.server.ts";
import { mapRevisions, maps, mapSources } from "#/db/global.ts";
import { generateMapId } from "./map_id.ts";
import { awbrnMapDocumentSchema, importedMapDocumentSchema } from "./map_document.ts";
import type { AwbrnMapDocument, ImportedMapDocument } from "./map_document.ts";
import type { MapRef } from "./schemas.ts";
import {
  canonicalizeAwbwMap,
  renderFullMapScreenshotPng,
  renderSmallMapScreenshotPng,
} from "#/server_wasm.ts";
import {
  MAP_SCREENSHOT_CACHE_CONTROL,
  MAP_SCREENSHOT_CONTENT_TYPE,
  MAP_SCREENSHOT_KINDS,
  mapScreenshotKey,
  type MapScreenshotKind,
} from "./map_screenshot.ts";

const db = drizzle(env.DB, { schema: { maps, mapSources, mapRevisions } });

export async function importAwbwMap(sourceMapId: number): Promise<MapRef> {
  if (!Number.isSafeInteger(sourceMapId) || sourceMapId <= 0)
    throw new Error("Invalid AWBW map id");
  const existing = await findAwbwMap(sourceMapId);
  if (existing) return existing;

  const imported = importedMapDocumentSchema.parse(
    canonicalizeAwbwMap(await fetchAwbwMapData(sourceMapId)),
  );
  const { document } = imported;
  const mapId = generateMapId();
  const now = new Date();
  await env.CONTENT.put(
    contentKey(imported.contentHash),
    JSON.stringify({
      width: document.width,
      height: document.height,
      terrain: document.terrain,
      units: document.units,
    }),
    { httpMetadata: { contentType: "application/json" } },
  );
  await storeMapScreenshots(imported);

  try {
    await db.batch([
      db.insert(maps).values({
        id: mapId,
        name: document.metadata.name,
        author: document.metadata.author,
        currentRevision: 1,
        createdAt: now,
        updatedAt: now,
      }),
      db.insert(mapSources).values({ mapId, source: "awbw", sourceMapId }),
      db.insert(mapRevisions).values({
        mapId,
        revision: 1,
        contentHash: imported.contentHash,
        width: document.width,
        height: document.height,
        playerCount: document.metadata.player_count,
        propertySignature: imported.propertySignature,
        unitSignature: imported.unitSignature,
        createdAt: now,
        lastSeenAt: now,
      }),
    ]);
    return { mapId, revision: 1 };
  } catch (error) {
    const raced = await findAwbwMap(sourceMapId);
    if (raced) return raced;
    throw error;
  }
}

/** How each picture of a map is drawn. */
const MAP_SCREENSHOT_RENDERERS: Record<
  MapScreenshotKind,
  (document: AwbrnMapDocument) => Uint8Array | Promise<Uint8Array>
> = {
  full: renderFullMapScreenshotPng,
  small: renderSmallMapScreenshotPng,
};

/**
 * Draw both pictures of the map and store them beside its document.
 *
 * A picture that cannot be stored fails the import. Nothing is written to the
 * database until both are in the bucket, so a map is never recorded without
 * its pictures, and the caller can import it again to try once more. Storing
 * them first is what makes that retry safe: both keys name the content hash,
 * so a second attempt rewrites the same bytes to the same places.
 */
async function storeMapScreenshots(imported: ImportedMapDocument): Promise<void> {
  await Promise.all(MAP_SCREENSHOT_KINDS.map((kind) => storeMapScreenshot(imported, kind)));
}

async function storeMapScreenshot(
  imported: ImportedMapDocument,
  kind: MapScreenshotKind,
): Promise<void> {
  try {
    const png = await MAP_SCREENSHOT_RENDERERS[kind](imported.document);
    await env.CONTENT.put(mapScreenshotKey(imported.contentHash, kind), png, {
      httpMetadata: {
        contentType: MAP_SCREENSHOT_CONTENT_TYPE,
        cacheControl: MAP_SCREENSHOT_CACHE_CONTROL,
      },
    });
  } catch (error) {
    throw new Error(`could not store the ${kind} picture of the map ${imported.contentHash}`, {
      cause: error,
    });
  }
}

export async function loadMapRevision({ mapId, revision }: MapRef) {
  const row = await db
    .select({
      contentHash: mapRevisions.contentHash,
      playerCount: mapRevisions.playerCount,
      name: maps.name,
      author: maps.author,
    })
    .from(mapRevisions)
    .innerJoin(maps, eq(maps.id, mapRevisions.mapId))
    .where(and(eq(mapRevisions.mapId, mapId), eq(mapRevisions.revision, revision)))
    .get();
  if (!row) throw new Error("Map revision not found");
  const object = await env.CONTENT.get(contentKey(row.contentHash));
  if (!object) throw new Error("Map content not found");
  const content = (await object.json()) as Record<string, unknown>;
  return awbrnMapDocumentSchema.parse({
    map_format: 1,
    ...content,
    metadata: { name: row.name, author: row.author, player_count: row.playerCount },
  });
}

async function findAwbwMap(sourceMapId: number): Promise<MapRef | null> {
  const row = await db
    .select({ mapId: mapSources.mapId, revision: maps.currentRevision })
    .from(mapSources)
    .innerJoin(maps, eq(maps.id, mapSources.mapId))
    .where(and(eq(mapSources.source, "awbw"), eq(mapSources.sourceMapId, sourceMapId)))
    .get();
  return row ? { mapId: row.mapId, revision: row.revision } : null;
}

function contentKey(hash: string): string {
  return `maps/doc/v1/${hash}`;
}

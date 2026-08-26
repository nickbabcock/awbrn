import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import { and, desc, eq, inArray, lt, or, sql } from "drizzle-orm";
import { fetchAwbwMapData } from "#/awbw/awbw.server.ts";
import type { AwbwMapData } from "#/awbw/schemas.ts";
import { mapRevisions, maps, mapSources, mapTags } from "#/db/global.ts";
import { generateMapId } from "./map_id.ts";
import { awbrnMapDocumentSchema, importedMapDocumentSchema } from "./map_document.ts";
import type { AwbrnMapDocument, ImportedMapDocument } from "./map_document.ts";
import type {
  MapCatalogEntry,
  MapCatalogRequest,
  MapCatalogResponse,
  MapRank,
  MapRef,
  MapSourceKind,
  MapTag,
} from "./schemas.ts";
import { sortMapTags } from "./map_taxonomy.ts";
import {
  decodeMapCatalogCursor,
  encodeMapCatalogCursor,
  MAP_CATALOG_PAGE_SIZE,
  mapSearchPattern,
  normalizeMapSearch,
} from "./map_catalog.ts";
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
  mapScreenshotPath,
  type MapScreenshotKind,
} from "./map_screenshot.ts";

const db = drizzle(env.DB, { schema: { maps, mapSources, mapRevisions, mapTags } });

export async function importAwbwMap(sourceMapId: number): Promise<MapRef> {
  if (!Number.isSafeInteger(sourceMapId) || sourceMapId <= 0)
    throw new Error("Invalid AWBW map id");
  const existing = await findAwbwMap(sourceMapId);
  if (existing) return existing;

  return storeAwbwMap(sourceMapId, await fetchAwbwMapData(sourceMapId));
}

/**
 * Put an AWBW map that is already in hand in the catalog.
 *
 * The map is stored as it stands, so the caller decides where the data comes
 * from: the live AWBW site, or a file held in the repository.
 */
export async function storeAwbwMap(sourceMapId: number, data: AwbwMapData): Promise<MapRef> {
  const existing = await findAwbwMap(sourceMapId);
  if (existing) return existing;

  const imported = importedMapDocumentSchema.parse(canonicalizeAwbwMap(data));
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

/**
 * A page of the catalog, newest map first.
 *
 * Every row is a map at its current revision, with the addresses of the two
 * pictures that revision was drawn into. One extra row is read to learn
 * whether a page follows this one.
 */
export async function listCatalogMaps(request: MapCatalogRequest): Promise<MapCatalogResponse> {
  const cursor = decodeMapCatalogCursor(request.cursor);
  const cursorCreatedAt = cursor ? new Date(cursor.createdAt) : null;
  const afterCursor =
    cursor && cursorCreatedAt && !Number.isNaN(cursorCreatedAt.getTime())
      ? or(
          lt(maps.createdAt, cursorCreatedAt),
          and(eq(maps.createdAt, cursorCreatedAt), lt(maps.id, cursor.mapId)),
        )
      : undefined;

  const search = normalizeMapSearch(request.search);
  // The pattern escapes the wildcards of `LIKE`, which only counts when the
  // statement says which character does the escaping.
  const pattern = search ? mapSearchPattern(search.toLowerCase()) : null;
  const matchesSearch = pattern
    ? or(
        sql`lower(${maps.name}) like ${pattern} escape '\\'`,
        sql`lower(${maps.author}) like ${pattern} escape '\\'`,
      )
    : undefined;

  const rows = await db
    .select({
      mapId: maps.id,
      name: maps.name,
      author: maps.author,
      revision: maps.currentRevision,
      createdAt: maps.createdAt,
      contentHash: mapRevisions.contentHash,
      width: mapRevisions.width,
      height: mapRevisions.height,
      playerCount: mapRevisions.playerCount,
      rank: mapRevisions.rank,
      source: mapSources.source,
      sourceMapId: mapSources.sourceMapId,
    })
    .from(maps)
    .innerJoin(
      mapRevisions,
      and(eq(mapRevisions.mapId, maps.id), eq(mapRevisions.revision, maps.currentRevision)),
    )
    .leftJoin(mapSources, eq(mapSources.mapId, maps.id))
    .where(and(afterCursor, matchesSearch))
    .orderBy(desc(maps.createdAt), desc(maps.id))
    .limit(MAP_CATALOG_PAGE_SIZE + 1)
    .all();

  const hasNextPage = rows.length > MAP_CATALOG_PAGE_SIZE;
  const visibleRows = hasNextPage ? rows.slice(0, MAP_CATALOG_PAGE_SIZE) : rows;
  const lastVisibleRow = visibleRows[visibleRows.length - 1] ?? null;
  const tags = await readMapTags(visibleRows.map((row) => row.mapId));

  return {
    maps: visibleRows.map((row) => toCatalogEntry(row, tags.get(row.mapId) ?? [])),
    pageSize: MAP_CATALOG_PAGE_SIZE,
    hasNextPage,
    nextCursor:
      hasNextPage && lastVisibleRow
        ? encodeMapCatalogCursor({
            createdAt: lastVisibleRow.createdAt.toISOString(),
            mapId: lastVisibleRow.mapId,
          })
        : null,
  };
}

/**
 * Put an AWBW map in the catalog and report the entry the catalog now holds.
 *
 * A map that is already held is returned as it stands, so importing the same
 * map twice is the same as looking it up.
 */
export async function importAwbwMapToCatalog(sourceMapId: number): Promise<MapCatalogEntry> {
  const ref = await importAwbwMap(sourceMapId);
  const entry = await findCatalogEntry(ref);
  if (!entry) throw new Error("the imported map is not in the catalog");
  return entry;
}

/** One catalog entry, or null when no such revision is held. */
export async function findCatalogEntry({
  mapId,
  revision,
}: MapRef): Promise<MapCatalogEntry | null> {
  const row = await db
    .select({
      mapId: maps.id,
      name: maps.name,
      author: maps.author,
      revision: mapRevisions.revision,
      createdAt: maps.createdAt,
      contentHash: mapRevisions.contentHash,
      width: mapRevisions.width,
      height: mapRevisions.height,
      playerCount: mapRevisions.playerCount,
      rank: mapRevisions.rank,
      source: mapSources.source,
      sourceMapId: mapSources.sourceMapId,
    })
    .from(mapRevisions)
    .innerJoin(maps, eq(maps.id, mapRevisions.mapId))
    .leftJoin(mapSources, eq(mapSources.mapId, maps.id))
    .where(and(eq(mapRevisions.mapId, mapId), eq(mapRevisions.revision, revision)))
    .get();

  if (!row) return null;
  const tags = await readMapTags([row.mapId]);
  return toCatalogEntry(row, tags.get(row.mapId) ?? []);
}

/**
 * The tags of every named map, in vocabulary order.
 *
 * Maps with no tags are left out of the result, so read it with a default of
 * an empty list.
 */
async function readMapTags(mapIds: readonly string[]): Promise<Map<string, MapTag[]>> {
  const tags = new Map<string, MapTag[]>();
  if (mapIds.length === 0) return tags;

  const rows = await db
    .select({ mapId: mapTags.mapId, tag: mapTags.tag })
    .from(mapTags)
    .where(inArray(mapTags.mapId, [...mapIds]))
    .all();

  for (const row of rows) {
    const held = tags.get(row.mapId);
    if (held) held.push(row.tag);
    else tags.set(row.mapId, [row.tag]);
  }
  for (const [mapId, held] of tags) tags.set(mapId, sortMapTags(held));
  return tags;
}

/**
 * Give a map revision a rank, or take away the rank it holds.
 *
 * The rank names one revision and not the map, so the next revision of the
 * map starts unranked however good this one was.
 */
export async function setMapRevisionRank(
  { mapId, revision }: MapRef,
  rank: MapRank | null,
): Promise<void> {
  const updated = await db
    .update(mapRevisions)
    .set({ rank })
    .where(and(eq(mapRevisions.mapId, mapId), eq(mapRevisions.revision, revision)))
    .returning({ mapId: mapRevisions.mapId })
    .all();
  if (updated.length === 0) throw new Error("Map revision not found");
}

/**
 * Replace every tag on a map with the tags named here.
 *
 * The whole set is written at once, because a tag that is taken off a map and
 * a tag that is put on it are one change to the player who made it.
 */
export async function setMapTags(mapId: string, tags: readonly MapTag[]): Promise<MapTag[]> {
  const wanted = sortMapTags(tags);
  const map = await db.select({ id: maps.id }).from(maps).where(eq(maps.id, mapId)).get();
  if (!map) throw new Error("Map not found");

  const clear = db.delete(mapTags).where(eq(mapTags.mapId, mapId));
  if (wanted.length === 0) {
    await clear;
    return wanted;
  }

  const now = new Date();
  await db.batch([
    clear,
    db.insert(mapTags).values(wanted.map((tag) => ({ mapId, tag, addedAt: now }))),
  ]);
  return wanted;
}

interface CatalogRow {
  mapId: string;
  name: string;
  author: string;
  revision: number;
  createdAt: Date;
  contentHash: string;
  width: number;
  height: number;
  playerCount: number;
  rank: MapRank | null;
  source: MapSourceKind | null;
  sourceMapId: number | null;
}

function toCatalogEntry(row: CatalogRow, tags: MapTag[]): MapCatalogEntry {
  return {
    mapId: row.mapId,
    revision: row.revision,
    name: row.name,
    author: row.author,
    playerCount: row.playerCount,
    rank: row.rank,
    tags,
    width: row.width,
    height: row.height,
    origin:
      row.source && row.sourceMapId !== null
        ? { kind: row.source, sourceMapId: row.sourceMapId }
        : null,
    screenshot: {
      small: mapScreenshotPath(row.contentHash, "small"),
      full: mapScreenshotPath(row.contentHash, "full"),
    },
    addedAt: row.createdAt.toISOString(),
  };
}

/**
 * The stored picture of a map revision, ready to serve.
 *
 * The picture is immutable and its address names the content it was drawn
 * from, so it is served with a year of cache and an entity tag.
 */
export async function readMapScreenshot(
  contentHash: string,
  kind: MapScreenshotKind,
): Promise<Response> {
  const object = await env.CONTENT.get(mapScreenshotKey(contentHash, kind));
  if (!object) return new Response("Map picture not found", { status: 404 });

  return new Response(object.body, {
    headers: {
      "Content-Type": MAP_SCREENSHOT_CONTENT_TYPE,
      "Cache-Control": MAP_SCREENSHOT_CACHE_CONTROL,
      ETag: object.httpEtag,
    },
  });
}

/**
 * The filter conditions, run against a real database.
 *
 * The statement is built by Drizzle and executed by SQLite over the same
 * migration the deployed catalog runs, so a condition that names a column
 * wrongly or counts tags wrongly fails here rather than on the board.
 */

import { env } from "cloudflare:workers";
import { and, desc, eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { beforeAll, describe, expect, it } from "vitest";
import { mapRevisions, mapTags, maps } from "#/db/global.ts";
import { catalogFilterConditions } from "./map_catalog_query.ts";
import { normalizeMapCatalogFilters } from "./map_taxonomy.ts";
import type { MapCatalogFilter } from "./schemas.ts";

/** The maps the scratch catalog holds, one per shape worth filtering for. */
const SEED = [
  { id: "aaaaaaaaaaaa", name: "Amber Valley", players: 2, rank: "S", tags: ["standard"] },
  { id: "bbbbbbbbbbbb", name: "Fog Harbor", players: 2, rank: "A", tags: ["standard", "fog"] },
  { id: "cccccccccccc", name: "Four Corners", players: 4, rank: null, tags: ["ffa"] },
  { id: "dddddddddddd", name: "Grand Melee", players: 6, rank: "B", tags: ["ffa", "high-funds"] },
] as const;

let db: ReturnType<typeof drizzle>;

/** The map names a filter set leaves on the board, newest first. */
function boardFor(filters: MapCatalogFilter): Promise<string[]> {
  return db
    .select({ name: maps.name })
    .from(maps)
    .innerJoin(
      mapRevisions,
      and(eq(mapRevisions.mapId, maps.id), eq(mapRevisions.revision, maps.currentRevision)),
    )
    .where(and(...catalogFilterConditions(normalizeMapCatalogFilters(filters))))
    .orderBy(desc(maps.id))
    .all()
    .then((rows) => rows.map((row) => row.name));
}

beforeAll(async () => {
  db = drizzle(env.DB);

  const now = new Date();
  await db.insert(maps).values(
    SEED.map((map, index) => ({
      id: map.id,
      name: map.name,
      author: "Bamboozle",
      currentRevision: 1,
      createdAt: new Date(now.getTime() + index),
      updatedAt: new Date(now.getTime() + index),
    })),
  );
  await db.insert(mapRevisions).values(
    SEED.map((map) => ({
      mapId: map.id,
      revision: 1,
      contentHash: `hash-${map.id}`,
      width: 20,
      height: 20,
      playerCount: map.players,
      propertySignature: "",
      unitSignature: "",
      rank: map.rank,
      createdAt: now,
      lastSeenAt: now,
    })),
  );
  await db.insert(mapTags).values(
    SEED.flatMap((map) =>
      map.tags.map((tag) => ({
        mapId: map.id,
        tag,
        addedAt: now,
      })),
    ),
  );
});

describe("map catalog filter conditions", () => {
  it("leaves the whole board when nothing is pressed", async () => {
    await expect(boardFor({})).resolves.toHaveLength(SEED.length);
  });

  it("reads player counts as any of them, and 5+ as everything above", async () => {
    await expect(boardFor({ playerCounts: ["2"] })).resolves.toEqual([
      "Fog Harbor",
      "Amber Valley",
    ]);
    await expect(boardFor({ playerCounts: ["4", "5+"] })).resolves.toEqual([
      "Grand Melee",
      "Four Corners",
    ]);
  });

  it("reads tags as all of them", async () => {
    await expect(boardFor({ tags: ["standard"] })).resolves.toEqual(["Fog Harbor", "Amber Valley"]);
    await expect(boardFor({ tags: ["standard", "fog"] })).resolves.toEqual(["Fog Harbor"]);
    await expect(boardFor({ tags: ["standard", "ffa"] })).resolves.toEqual([]);
  });

  it("finds the maps that hold no rank", async () => {
    await expect(boardFor({ ranks: ["unranked"] })).resolves.toEqual(["Four Corners"]);
    await expect(boardFor({ ranks: ["S", "unranked"] })).resolves.toEqual([
      "Four Corners",
      "Amber Valley",
    ]);
  });

  it("narrows once for every question that was asked", async () => {
    await expect(boardFor({ playerCounts: ["2"], ranks: ["A"], tags: ["fog"] })).resolves.toEqual([
      "Fog Harbor",
    ]);
    await expect(boardFor({ playerCounts: ["4"], tags: ["fog"] })).resolves.toEqual([]);
  });
});

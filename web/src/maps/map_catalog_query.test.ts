/**
 * The filter conditions, run against a real database.
 *
 * The statement is built by Drizzle and executed by SQLite over the same
 * migration the deployed catalog runs, so a condition that names a column
 * wrongly or counts tags wrongly fails here rather than on the board.
 */

import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { and, desc, eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/sqlite-proxy";
import { beforeAll, describe, expect, it } from "vitest";
import { mapRevisions, maps } from "#/db/global.ts";
import { catalogFilterConditions } from "./map_catalog_query.ts";
import { normalizeMapCatalogFilters } from "./map_taxonomy.ts";
import type { MapCatalogFilter } from "./schemas.ts";

const MIGRATION = new URL("../../drizzle/global/0000_initial.sql", import.meta.url);

/** The maps the scratch catalog holds, one per shape worth filtering for. */
const SEED = [
  { id: "aaaaaaaaaaaa", name: "Amber Valley", players: 2, rank: "S", tags: ["standard"] },
  { id: "bbbbbbbbbbbb", name: "Fog Harbor", players: 2, rank: "A", tags: ["standard", "fog"] },
  { id: "cccccccccccc", name: "Four Corners", players: 4, rank: null, tags: ["ffa"] },
  { id: "dddddddddddd", name: "Grand Melee", players: 6, rank: "B", tags: ["ffa", "high-funds"] },
] as const;

let sqlite: DatabaseSync;
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

beforeAll(() => {
  sqlite = new DatabaseSync(":memory:");
  // The migration is written as one file of statements separated by Drizzle's
  // own breakpoint marker.
  for (const statement of readFileSync(MIGRATION, "utf8").split("--> statement-breakpoint")) {
    if (statement.trim().length > 0) sqlite.exec(statement);
  }

  db = drizzle(async (query, params) => {
    const rows = sqlite.prepare(query).all(...(params as never[]));
    return { rows: rows.map((row) => Object.values(row as object)) };
  });

  const now = Date.now();
  for (const [index, map] of SEED.entries()) {
    sqlite
      .prepare(
        "insert into maps (id, name, author, currentRevision, createdAt, updatedAt) values (?, ?, ?, 1, ?, ?)",
      )
      .run(map.id, map.name, "Bamboozle", now + index, now + index);
    sqlite
      .prepare(
        `insert into map_revisions
           (mapId, revision, contentHash, width, height, playerCount, propertySignature, unitSignature, rank, createdAt, lastSeenAt)
         values (?, 1, ?, 20, 20, ?, '', '', ?, ?, ?)`,
      )
      .run(map.id, `hash-${map.id}`, map.players, map.rank, now, now);
    for (const tag of map.tags) {
      sqlite
        .prepare("insert into map_tags (mapId, tag, addedAt) values (?, ?, ?)")
        .run(map.id, tag, now);
    }
  }
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

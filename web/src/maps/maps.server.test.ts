import { env } from "cloudflare:workers";
import { and, eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { actorFromRole } from "#/auth/actor.ts";
import { mapRevisions, maps, user } from "#/db/global.ts";
import { MAP_CATALOG_PAGE_SIZE } from "./map_catalog.ts";
import type { AwbrnMapDocument } from "./map_document.ts";
import { listCatalogMaps, loadMapRevision, saveEditedMap } from "./maps.server.ts";
import { MAP_SAVE_REFUSALS, mapSaveRequestSchema, type MapRank } from "./schemas.ts";

const db = drizzle(env.DB);

/**
 * More maps than a page holds. The rank B run straddles the first page
 * boundary, and its maps are added three to a second so the last map of the
 * page shares its age with the first map of the next one.
 */
const RUNS: { rank: MapRank | null; place: number; count: number }[] = [
  { rank: "S", place: 0, count: 6 },
  { rank: "A", place: 1, count: 6 },
  { rank: "B", place: 2, count: 16 },
  { rank: null, place: 4, count: 6 },
];

const SEED = RUNS.flatMap((run) =>
  Array.from({ length: run.count }, (_, index) => ({ ...run, second: Math.floor(index / 3) })),
).map((map, index, all) => ({
  ...map,
  // Ids that do not follow the insertion order, so only the tie-break orders them.
  id: `map${String((index * 11) % all.length).padStart(9, "0")}`,
}));

beforeAll(async () => {
  const epoch = Date.UTC(2026, 0, 1);
  // One map to a statement, since D1 caps the variables a statement binds.
  for (const map of SEED) {
    await db.insert(maps).values({
      id: map.id,
      name: map.id,
      author: "Bamboozle",
      currentRevision: 1,
      createdAt: new Date(epoch + map.second * 1000),
      updatedAt: new Date(epoch),
    });
    await db.insert(mapRevisions).values({
      mapId: map.id,
      revision: 1,
      contentHash: `hash-${map.id}`,
      width: 20,
      height: 20,
      playerCount: 2,
      propertySignature: "",
      unitSignature: "",
      rank: map.rank,
      createdAt: new Date(epoch),
      lastSeenAt: new Date(epoch),
    });
  }
});

describe("map catalog listing", () => {
  it("pages through every map once, best rank first, then newest, then highest id", async () => {
    const expected = SEED.toSorted(
      (left, right) =>
        left.place - right.place ||
        right.second - left.second ||
        (right.id < left.id ? -1 : right.id > left.id ? 1 : 0),
    );
    const [lastOfPage, firstOfNext] = expected.slice(
      MAP_CATALOG_PAGE_SIZE - 1,
      MAP_CATALOG_PAGE_SIZE + 1,
    );
    expect(lastOfPage).toMatchObject({ rank: "B", second: firstOfNext?.second });

    const seen: string[] = [];
    let cursor: string | undefined;
    let pages = 0;
    do {
      const page = await listCatalogMaps({ cursor });
      seen.push(...page.maps.map((map) => map.mapId));
      cursor = page.nextCursor ?? undefined;
      pages += 1;
    } while (cursor && pages < 10);

    expect(pages).toBe(2);
    expect(seen).toEqual(expected.map((map) => map.id));
  });
});

function draft(terrainId = 1): AwbrnMapDocument {
  const terrain = Array.from({ length: 25 }, () => 1);
  terrain[0] = terrainId;
  return {
    map_format: 1,
    width: 5,
    height: 5,
    terrain,
    units: [],
    metadata: { name: "Draft", author: "Map maker", player_count: 0 },
  };
}

function serveAssets() {
  const assetsFetch = env.ASSETS.fetch.bind(env.ASSETS);
  vi.spyOn(env.ASSETS, "fetch").mockImplementation((input, init) => {
    const url = new URL(
      typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
    );
    const assetsPath = url.pathname.lastIndexOf("/assets/");
    if (assetsPath !== -1) {
      url.pathname = url.pathname.slice(assetsPath + "/assets".length);
    }
    return assetsFetch(url, init);
  });
}

describe("saving map drafts", () => {
  it("stores unfinished boards and rejects stale edits atomically", async () => {
    serveAssets();
    const userId = `map-save-${crypto.randomUUID()}`;
    const actor = actorFromRole(userId, "user");
    let mapId: string | undefined;
    try {
      await db.insert(user).values({
        id: userId,
        name: "Map maker",
        nameKey: userId,
        email: `${userId}@example.com`,
        emailVerified: true,
        updatedAt: new Date(),
      });

      const created = await saveEditedMap(
        mapSaveRequestSchema.parse({ name: "Draft", document: draft() }),
        actor,
      );
      mapId = created.mapId;

      expect(created).toMatchObject({ revision: 1, written: true });
      await expect(
        saveEditedMap(
          mapSaveRequestSchema.parse({
            mapId,
            expectedRevision: 1,
            expectedName: "Draft",
            name: "Taken",
            document: draft(2),
          }),
          actorFromRole(`map-save-other-${crypto.randomUUID()}`, "user"),
        ),
      ).rejects.toThrow(MAP_SAVE_REFUSALS.forbidden);
      await expect(loadMapRevision({ mapId, revision: 1 })).resolves.toMatchObject({
        metadata: { player_count: 0 },
      });

      const candidates = [draft(2), draft(3)];
      const results = await Promise.allSettled(
        candidates.map((document) =>
          saveEditedMap(
            mapSaveRequestSchema.parse({
              mapId,
              expectedRevision: 1,
              expectedName: "Draft",
              name: "Concurrent winner",
              document,
            }),
            actor,
          ),
        ),
      );
      const winnerIndex = results.findIndex((result) => result.status === "fulfilled");
      expect(winnerIndex).toBeGreaterThanOrEqual(0);
      expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
      expect(results.filter((result) => result.status === "rejected")).toHaveLength(1);

      const winnerDocument = candidates[winnerIndex]!;
      await expect(
        saveEditedMap(
          mapSaveRequestSchema.parse({
            mapId,
            expectedRevision: 1,
            expectedName: "Draft",
            name: "Stale content",
            document: candidates[1 - winnerIndex]!,
          }),
          actor,
        ),
      ).rejects.toThrow("changed after you opened it");

      await expect(
        saveEditedMap(
          mapSaveRequestSchema.parse({
            mapId,
            expectedRevision: 1,
            expectedName: "Draft",
            name: "Stale no-op",
            document: winnerDocument,
          }),
          actor,
        ),
      ).rejects.toThrow("changed after you opened it");

      const renamed = await saveEditedMap(
        mapSaveRequestSchema.parse({
          mapId,
          expectedRevision: 2,
          expectedName: "Concurrent winner",
          name: "Renamed draft",
          document: winnerDocument,
        }),
        actor,
      );
      expect(renamed).toMatchObject({ revision: 2, written: false });

      await expect(
        saveEditedMap(
          mapSaveRequestSchema.parse({
            mapId,
            expectedRevision: 2,
            expectedName: "Concurrent winner",
            name: "Stale rename",
            document: winnerDocument,
          }),
          actor,
        ),
      ).rejects.toThrow("changed after you opened it");

      const current = await db
        .select({ name: maps.name, currentRevision: maps.currentRevision })
        .from(maps)
        .where(eq(maps.id, mapId))
        .get();
      const revisions = await db
        .select({ revision: mapRevisions.revision })
        .from(mapRevisions)
        .where(and(eq(mapRevisions.mapId, mapId), eq(mapRevisions.revision, 2)))
        .all();

      expect(current).toEqual({ name: "Renamed draft", currentRevision: 2 });
      expect(revisions).toEqual([{ revision: 2 }]);
    } finally {
      vi.restoreAllMocks();
      if (mapId) await db.delete(maps).where(eq(maps.id, mapId));
      await db.delete(user).where(eq(user.id, userId));
    }
  });

  it("returns to the revision that already holds a saved board, rank and all", async () => {
    serveAssets();
    const userId = `map-revert-${crypto.randomUUID()}`;
    const actor = actorFromRole(userId, "user");
    let mapId: string | undefined;
    try {
      await db.insert(user).values({
        id: userId,
        name: "Map maker",
        nameKey: userId,
        email: `${userId}@example.com`,
        emailVerified: true,
        updatedAt: new Date(),
      });
      const revise = (expectedRevision: number, document: AwbrnMapDocument) =>
        saveEditedMap(
          mapSaveRequestSchema.parse({
            mapId,
            expectedRevision,
            expectedName: "Draft",
            name: "Draft",
            document,
          }),
          actor,
        );

      ({ mapId } = await saveEditedMap(
        mapSaveRequestSchema.parse({ name: "Draft", document: draft() }),
        actor,
      ));
      await db
        .update(mapRevisions)
        .set({ rank: "A" })
        .where(and(eq(mapRevisions.mapId, mapId), eq(mapRevisions.revision, 1)));
      await expect(revise(1, draft(2))).resolves.toMatchObject({ revision: 2, written: true });

      await expect(revise(2, draft())).resolves.toEqual({ mapId, revision: 1, written: false });

      const current = await db
        .select({ currentRevision: maps.currentRevision, rank: mapRevisions.rank })
        .from(maps)
        .innerJoin(
          mapRevisions,
          and(eq(mapRevisions.mapId, maps.id), eq(mapRevisions.revision, maps.currentRevision)),
        )
        .where(eq(maps.id, mapId))
        .get();
      expect(current).toEqual({ currentRevision: 1, rank: "A" });

      await expect(revise(1, draft(3))).resolves.toEqual({ mapId, revision: 3, written: true });
    } finally {
      vi.restoreAllMocks();
      if (mapId) await db.delete(maps).where(eq(maps.id, mapId));
      await db.delete(user).where(eq(user.id, userId));
    }
  });
});

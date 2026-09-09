import { env } from "cloudflare:workers";
import { and, eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import { describe, expect, it, vi } from "vitest";
import { actorFromRole } from "#/auth/actor.ts";
import { mapRevisions, maps, user } from "#/db/global.ts";
import type { AwbrnMapDocument } from "./map_document.ts";
import { loadMapRevision, saveEditedMap } from "./maps.server.ts";
import { mapSaveRequestSchema } from "./schemas.ts";

const db = drizzle(env.DB);

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

describe("saving map drafts", () => {
  it("stores unfinished boards and rejects stale edits atomically", async () => {
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

    const userId = `map-save-${crypto.randomUUID()}`;
    const actor = actorFromRole(userId, "user");
    let mapId: string | undefined;
    try {
      await db.insert(user).values({
        id: userId,
        name: "Map maker",
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
});

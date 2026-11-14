import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import { beforeAll, describe, expect, it } from "vitest";
import { mapRevisions, maps } from "#/db/global.ts";
import { MAP_CATALOG_PAGE_SIZE } from "./map_catalog.ts";
import { listCatalogMaps } from "./maps.server.ts";
import type { MapRank } from "./schemas.ts";

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
  const db = drizzle(env.DB);
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

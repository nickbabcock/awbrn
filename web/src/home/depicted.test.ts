import { describe, expect, it } from "vitest";
import type { MapCatalogEntry, MapRank } from "#/maps/schemas.ts";
import { byRank, chooseFirstMatchMap, chooseLandingMaps, placeSkirmish } from "./depicted.ts";

function map(
  mapId: string,
  rank: MapRank | null,
  playerCount: number,
  width: number,
  height: number,
): MapCatalogEntry {
  return {
    mapId,
    revision: 1,
    name: mapId,
    author: "someone",
    authorUserId: null,
    playerCount,
    rank,
    tags: [],
    width,
    height,
    origin: null,
    screenshot: { small: "", full: "" },
    addedAt: "2026-01-01T00:00:00.000Z",
  };
}

describe("byRank", () => {
  it("puts S first and keeps the catalog order inside a rank", () => {
    const maps = [
      map("b", "B", 2, 20, 20),
      map("a1", "A", 2, 20, 20),
      map("none", null, 2, 20, 20),
      map("s", "S", 2, 20, 20),
      map("a2", "A", 2, 20, 20),
    ];
    expect(byRank(maps).map((entry) => entry.mapId)).toEqual(["s", "a1", "a2", "b"]);
  });
});

describe("chooseLandingMaps", () => {
  it("features the first match's map, rows the next four, and replays another duel", () => {
    const maps = [
      map("ffa", "S", 4, 20, 20),
      map("duel-s", "S", 2, 20, 20),
      map("a1", "A", 4, 20, 20),
      map("a2", "A", 2, 20, 20),
      map("b", "B", 2, 20, 20),
      map("c", "C", 2, 20, 20),
    ];
    const { featured, replay, rows } = chooseLandingMaps(maps);
    expect(featured?.mapId).toBe("duel-s");
    expect(rows.map((entry) => entry.mapId)).toEqual(["ffa", "a1", "a2", "b"]);
    expect(replay?.mapId).toBe("a2");
  });

  it("features the strongest map when no duel is ranked, and replays it too", () => {
    const { featured, replay, rows } = chooseLandingMaps([map("ffa", "A", 4, 20, 20)]);
    expect(featured?.mapId).toBe("ffa");
    expect(rows).toEqual([]);
    expect(replay?.mapId).toBe("ffa");
  });
});

describe("chooseFirstMatchMap", () => {
  it("takes the S-rank duel nearest 20 by 20", () => {
    const maps = [
      map("big-s", "S", 2, 30, 30),
      map("ffa-s", "S", 4, 20, 20),
      map("a", "A", 2, 20, 20),
      map("near-s", "S", 2, 21, 19),
    ];
    expect(chooseFirstMatchMap(maps)?.mapId).toBe("near-s");
  });

  it("falls back to the strongest duel and never to a map for more armies", () => {
    expect(chooseFirstMatchMap([map("ffa", "S", 4, 20, 20), map("a", "A", 2, 25, 25)])?.mapId).toBe(
      "a",
    );
    expect(chooseFirstMatchMap([map("ffa", "B", 4, 20, 20)])).toBeNull();
    expect(chooseFirstMatchMap([map("unranked", null, 2, 20, 20)])).toBeNull();
  });
});

describe("placeSkirmish", () => {
  it("puts every unit on open ground that nothing else holds", () => {
    const width = 24;
    const height = 20;
    // Plains, with a sea column and a city in the middle of the formation.
    const terrain = Array.from({ length: width * height }, (_, index): number => {
      const x = index % width;
      if (x === 14) return 28;
      return 1;
    });
    terrain[10 * width + 9] = 34;
    const document = { width, height, terrain, units: [{ position: [8, 10] as [number, number] }] };

    const { focus, focusDefense, units } = placeSkirmish(document);
    expect(units.length).toBeGreaterThan(10);
    expect(units[0]).toMatchObject({ unit: "tank", side: 0, ...focus });
    expect(focusDefense).toBe(1);

    const spots = new Set<string>();
    for (const unit of units) {
      const terrainId = terrain[unit.y * width + unit.x];
      expect(terrainId).toBe(1);
      expect(`${unit.x},${unit.y}`).not.toBe("8,10");
      spots.add(`${unit.x},${unit.y}`);
    }
    expect(spots.size).toBe(units.length);
  });
});

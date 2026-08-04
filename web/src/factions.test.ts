import { describe, expect, it } from "vitest";
import type { AwbwMapData } from "#/awbw/schemas.ts";
import { mapSlotFactionIds } from "./factions.ts";

function mapWithTerrain(terrainIds: number[], playerCount = 2): AwbwMapData {
  return {
    Name: "Test map",
    Author: "Test author",
    "Player Count": playerCount,
    "Published Date": "2026-08-04 00:00:00",
    "Size X": terrainIds.length,
    "Size Y": 1,
    "Terrain Map": [terrainIds],
    "Predeployed Units": [],
  };
}

describe("mapSlotFactionIds", () => {
  it("uses the factions which own the map instead of the global catalog defaults", () => {
    const map = mapWithTerrain([53, 57, 86, 90]);

    expect(mapSlotFactionIds(map, 2)).toEqual([4, 7]);
  });

  it("recognizes factions represented only by predeployed units", () => {
    const map = mapWithTerrain([]);
    map["Predeployed Units"] = [
      { "Unit ID": 1, "Unit X": 0, "Unit Y": 0, "Unit HP": 10, "Country Code": "yc" },
      { "Unit ID": 1, "Unit X": 1, "Unit Y": 0, "Unit HP": 10, "Country Code": "gs" },
    ];

    expect(mapSlotFactionIds(map, 2)).toEqual([4, 7]);
  });

  it("falls back to the catalog when map ownership is incomplete", () => {
    const map = mapWithTerrain([1, 2, 3]);

    expect(mapSlotFactionIds(map, 2)).toEqual([1, 2]);
  });

  it("gives every slot a different faction when the map names too few", () => {
    // An Orange Star HQ and a Green Earth HQ on a map which claims four seats.
    const map = mapWithTerrain([42, 52], 4);

    const slotFactionIds = mapSlotFactionIds(map, 4);

    expect(slotFactionIds).toHaveLength(4);
    expect(new Set(slotFactionIds).size).toBe(4);
    expect(slotFactionIds.slice(0, 2)).toEqual([1, 3]);
    // The remainder comes from the catalog, less the factions already held.
    expect(slotFactionIds.slice(2)).toEqual([2, 4]);
  });

  it("ignores map factions past the seat count", () => {
    const map = mapWithTerrain([42, 47, 52, 57], 2);

    expect(mapSlotFactionIds(map, 2)).toEqual([1, 2]);
  });

  it("ignores neutral properties", () => {
    // 34 is a neutral city and 42 an Orange Star HQ.
    const map = mapWithTerrain([34, 42], 1);

    expect(mapSlotFactionIds(map, 1)).toEqual([1]);
  });
});

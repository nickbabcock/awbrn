import { describe, expect, it } from "vitest";
import {
  compareMapRanks,
  countMapCatalogFilters,
  isMapCatalogFilterEmpty,
  mapRankAtLeast,
  mapRankOrder,
  normalizeMapCatalogFilters,
  sortMapTags,
} from "./map_taxonomy.ts";
import { MAP_RANKS, MAP_TAGS, MAP_TAG_LABELS, mapRankSchema, mapTagSchema } from "./schemas.ts";

describe("map ranks", () => {
  it("runs from C at the bottom to S at the top", () => {
    expect(MAP_RANKS).toEqual(["C", "B", "A", "S"]);
    expect(mapRankOrder("C")).toBe(0);
    expect(mapRankOrder("S")).toBe(MAP_RANKS.length - 1);
  });

  it("sorts the best rank first and leaves the unranked last", () => {
    const ranks = ["B", null, "S", "C", "A"] as const;
    expect([...ranks].sort(compareMapRanks)).toEqual(["S", "A", "B", "C", null]);
  });

  it("reads a rank against a floor", () => {
    expect(mapRankAtLeast("A", "B")).toBe(true);
    expect(mapRankAtLeast("B", "B")).toBe(true);
    expect(mapRankAtLeast("C", "B")).toBe(false);
    // An unranked revision is below every rank, not above the bottom one.
    expect(mapRankAtLeast(null, "C")).toBe(false);
  });

  it("takes only the ranks it knows", () => {
    expect(mapRankSchema.safeParse("S").success).toBe(true);
    expect(mapRankSchema.safeParse("D").success).toBe(false);
    expect(mapRankSchema.safeParse("s").success).toBe(false);
  });
});

describe("map tags", () => {
  it("names every tag it holds", () => {
    for (const tag of MAP_TAGS) expect(MAP_TAG_LABELS[tag]).toBeTruthy();
    expect(MAP_TAG_LABELS.ffa).toBe("FFA");
    expect(MAP_TAG_LABELS["high-funds"]).toBe("High funds");
  });

  it("puts the tags of a map in vocabulary order", () => {
    expect(sortMapTags(["fog", "standard"])).toEqual(["standard", "fog"]);
    expect(sortMapTags([...MAP_TAGS].reverse())).toEqual([...MAP_TAGS]);
  });

  it("holds each tag once and keeps an untagged map untagged", () => {
    expect(sortMapTags(["team", "team", "ffa"])).toEqual(["team", "ffa"]);
    expect(sortMapTags([])).toEqual([]);
  });

  it("takes only the tags it knows", () => {
    expect(mapTagSchema.safeParse("high-funds").success).toBe(true);
    expect(mapTagSchema.safeParse("High funds").success).toBe(false);
  });
});

describe("map catalog filters", () => {
  it("leaves the board wide when nothing is pressed", () => {
    expect(normalizeMapCatalogFilters(undefined)).toEqual({
      playerCounts: [],
      ranks: [],
      tags: [],
    });
    expect(isMapCatalogFilterEmpty(normalizeMapCatalogFilters(null))).toBe(true);
  });

  it("writes each list once and in vocabulary order", () => {
    expect(normalizeMapCatalogFilters({ tags: ["fog", "standard", "fog"] }).tags).toEqual([
      "standard",
      "fog",
    ]);
    expect(normalizeMapCatalogFilters({ ranks: ["A", "S"] }).ranks).toEqual(["S", "A"]);
  });

  it("reads a filter that names every answer as no filter at all", () => {
    expect(
      normalizeMapCatalogFilters({ playerCounts: ["4", "2", "3", "5+"] }).playerCounts,
    ).toEqual([]);
  });

  it("counts the buttons a player pressed", () => {
    expect(
      countMapCatalogFilters(normalizeMapCatalogFilters({ playerCounts: ["2"], tags: ["fog"] })),
    ).toBe(2);
  });
});

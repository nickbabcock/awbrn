import { describe, expect, it } from "vitest";
import { compareMapRanks, mapRankAtLeast, mapRankOrder, sortMapTags } from "./map_taxonomy.ts";
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

import { describe, expect, it } from "vitest";
import {
  mapBoardAddress,
  mapBoardFilters,
  mapBoardSearchText,
  validateMapBoardSearch,
} from "./map_board_search.ts";

describe("validateMapBoardSearch", () => {
  it("keeps only values the vocabularies hold", () => {
    expect(validateMapBoardSearch({ armies: "2,9,4", rank: "S,Z", tags: "fog,bogus" })).toEqual({
      armies: "2,4",
      rank: "S",
      tags: "fog",
    });
  });

  it("leaves out a row nothing survives from", () => {
    expect(validateMapBoardSearch({ armies: "", rank: "nope", q: "   " })).toEqual({});
  });

  it("puts a row in vocabulary order however it was written", () => {
    expect(validateMapBoardSearch({ rank: "unranked,S,A" }).rank).toBe("S,A,unranked");
  });
});

describe("mapBoardFilters", () => {
  it("reads the three rows of an address", () => {
    const search = validateMapBoardSearch({ armies: "4,2", tags: "team,fog", rank: "A" });
    expect(mapBoardFilters(search)).toEqual({
      playerCounts: ["2", "4"],
      tags: ["fog", "team"],
      ranks: ["A"],
    });
  });

  it("reads an empty address as a board nothing is pressed on", () => {
    expect(mapBoardFilters({})).toEqual({ playerCounts: [], tags: [], ranks: [] });
  });
});

describe("mapBoardAddress", () => {
  it("leaves every empty part out", () => {
    expect(mapBoardAddress("  ", { playerCounts: [], ranks: [], tags: [] })).toEqual({});
  });

  it("survives a round trip through the address", () => {
    const written = {
      ...mapBoardAddress("rome", { playerCounts: ["2"], ranks: ["S"], tags: ["fog"] }),
    };
    expect(mapBoardSearchText(validateMapBoardSearch(written))).toBe("rome");
    expect(mapBoardFilters(validateMapBoardSearch(written))).toEqual({
      playerCounts: ["2"],
      ranks: ["S"],
      tags: ["fog"],
    });
  });
});

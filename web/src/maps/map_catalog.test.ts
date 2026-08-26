import { describe, expect, it } from "vitest";
import {
  decodeMapCatalogCursor,
  encodeMapCatalogCursor,
  mapSearchPattern,
  MAP_SEARCH_MAX_LENGTH,
  normalizeMapSearch,
} from "./map_catalog.ts";

describe("map catalog paging", () => {
  const cursor = { createdAt: "2026-08-25T12:00:00.000Z", mapId: "abc123def456" };

  it("reads back a cursor it wrote", () => {
    expect(decodeMapCatalogCursor(encodeMapCatalogCursor(cursor))).toEqual(cursor);
  });

  it("refuses a cursor that names no page", () => {
    expect(decodeMapCatalogCursor(undefined)).toBeNull();
    expect(decodeMapCatalogCursor("not json")).toBeNull();
    expect(decodeMapCatalogCursor(JSON.stringify({ createdAt: cursor.createdAt }))).toBeNull();
  });

  it("refuses a cursor whose map id could not exist", () => {
    expect(decodeMapCatalogCursor(JSON.stringify({ ...cursor, mapId: "short" }))).toBeNull();
    expect(decodeMapCatalogCursor(JSON.stringify({ ...cursor, mapId: "ABC123DEF456" }))).toBeNull();
  });
});

describe("map catalog search", () => {
  it("lists everything when nothing is searched for", () => {
    expect(normalizeMapSearch(undefined)).toBeNull();
    expect(normalizeMapSearch("")).toBeNull();
    expect(normalizeMapSearch("   ")).toBeNull();
  });

  it("collapses the space a player types", () => {
    expect(normalizeMapSearch("  amber   valley ")).toBe("amber valley");
  });

  it("cuts search text that is longer than the catalog reads", () => {
    expect(normalizeMapSearch("a".repeat(200))).toHaveLength(MAP_SEARCH_MAX_LENGTH);
  });

  it("searches for a wildcard as text", () => {
    expect(mapSearchPattern("100%")).toBe("%100\\%%");
    expect(mapSearchPattern("a_b")).toBe("%a\\_b%");
    expect(mapSearchPattern("back\\slash")).toBe("%back\\\\slash%");
  });
});

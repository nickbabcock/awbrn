import { describe, expect, it } from "vitest";
import {
  MATCH_BROWSE_PAGE_SIZE,
  decodeMatchBrowseCursor,
  encodeMatchBrowseCursor,
} from "./match_browse.ts";

describe("cursor encoding", () => {
  it("round-trips a valid browse cursor", () => {
    const encoded = encodeMatchBrowseCursor({
      createdAt: "2026-04-06T12:00:00.000Z",
      matchId: "abc123",
    });

    expect(decodeMatchBrowseCursor(encoded)).toEqual({
      createdAt: "2026-04-06T12:00:00.000Z",
      matchId: "abc123",
    });
  });

  it("rejects malformed cursors", () => {
    expect(decodeMatchBrowseCursor(undefined)).toBeNull();
    expect(decodeMatchBrowseCursor("not-json")).toBeNull();
    expect(decodeMatchBrowseCursor(JSON.stringify({ createdAt: "x" }))).toBeNull();
  });
});

describe("page size", () => {
  it("keeps a stable browse page size", () => {
    expect(MATCH_BROWSE_PAGE_SIZE).toBe(12);
  });
});

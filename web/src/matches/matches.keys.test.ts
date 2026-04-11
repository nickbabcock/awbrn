import { describe, expect, it } from "vitest";
import { matchKeys, normalizeJoinSlug } from "./matches.keys";

describe("match query keys", () => {
  it("normalizes absent join slugs", () => {
    expect(normalizeJoinSlug(undefined)).toBeNull();
    expect(normalizeJoinSlug(null)).toBeNull();
    expect(normalizeJoinSlug("")).toBeNull();
  });

  it("keeps stable hierarchical keys", () => {
    expect(matchKeys.browse()).toEqual(["matches", "browse"]);
    expect(matchKeys.mine()).toEqual(["matches", "mine"]);
    expect(matchKeys.detail("abc123", undefined)).toEqual(["matches", "detail", "abc123", null]);
    expect(matchKeys.detail("abc123", "invite")).toEqual(["matches", "detail", "abc123", "invite"]);
  });
});

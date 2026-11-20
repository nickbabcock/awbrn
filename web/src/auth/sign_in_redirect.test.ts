import { describe, expect, it } from "vitest";
import { signInReturn } from "./sign_in_redirect.ts";

describe("where sign-in returns to", () => {
  it("returns to a path on this site", () => {
    expect(signInReturn("/maps/new?from=s7nsjmlzp9o3")).toBe("/maps/new?from=s7nsjmlzp9o3");
  });

  it("never leaves the site", () => {
    for (const value of [
      "//evil.com",
      "/\\evil.com",
      "/\\/evil.com",
      "https://evil.com",
      "evil.com",
    ]) {
      expect(signInReturn(value)).toBeUndefined();
    }
    expect(signInReturn(undefined)).toBeUndefined();
  });
});

import { describe, expect, it } from "vitest";
import { isBanned } from "./ban.ts";

const now = new Date("2026-08-26T12:00:00.000Z");

describe("isBanned", () => {
  it("holds an unexpiring ban", () => {
    expect(isBanned(true, null, now)).toBe(true);
    expect(isBanned(true, undefined, now)).toBe(true);
  });

  it("holds a ban until the moment it expires", () => {
    expect(isBanned(true, new Date("2026-08-26T12:00:01.000Z"), now)).toBe(true);
    expect(isBanned(true, new Date("2026-08-26T12:00:00.000Z"), now)).toBe(false);
    expect(isBanned(true, new Date("2026-08-25T12:00:00.000Z"), now)).toBe(false);
  });

  it("reads an empty column as no ban", () => {
    expect(isBanned(false, null, now)).toBe(false);
    expect(isBanned(null, null, now)).toBe(false);
    expect(isBanned(undefined, new Date("2027-01-01T00:00:00.000Z"), now)).toBe(false);
  });
});

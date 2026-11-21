import { describe, expect, it } from "vitest";
import { displayNameKey, displayNameSchema } from "./display_name.ts";

const accepts = (raw: string) => displayNameSchema.safeParse(raw).success;

describe("displayNameSchema", () => {
  it("stores the name trimmed, with one space between words", () => {
    expect(displayNameSchema.parse("  Max   Strike ")).toBe("Max Strike");
  });

  it("takes letters in any script", () => {
    expect(accepts("Sami_88")).toBe(true);
    expect(accepts("ネル司令")).toBe(true);
    expect(accepts("Grit-o.b")).toBe(true);
  });

  it("turns away names out of bounds or made of separators", () => {
    expect(accepts("ab")).toBe(false);
    expect(accepts("a".repeat(21))).toBe(false);
    expect(accepts("---")).toBe(false);
    expect(accepts("Hawke 🦅")).toBe(false);
    expect(accepts("<b>Kanbei</b>")).toBe(false);
  });

  it("keeps staff names and CPU labels for the site", () => {
    expect(accepts("Admin")).toBe(false);
    expect(accepts("mod.erator")).toBe(false);
    expect(accepts("Hard CPU")).toBe(false);
    expect(accepts("cpu")).toBe(false);
    expect(accepts("CPU Killer")).toBe(true);
  });
});

describe("displayNameKey", () => {
  it("folds case, width and separators into one name", () => {
    const key = displayNameKey("Max Strike");
    expect(displayNameKey("max_strike")).toBe(key);
    expect(displayNameKey("MAX.STRIKE")).toBe(key);
    expect(displayNameKey("Ｍａｘ Ｓｔｒｉｋｅ")).toBe(key);
    expect(displayNameKey("Max Strike2")).not.toBe(key);
  });
});

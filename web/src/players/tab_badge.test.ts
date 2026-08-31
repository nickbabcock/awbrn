import { describe, expect, it } from "vitest";
import { countLabel, faviconDataUrl, tabTitle } from "./tab_badge.ts";

describe("tabTitle", () => {
  it("leaves the title alone when nothing is waiting", () => {
    expect(tabTitle("AWBRN", 0)).toBe("AWBRN");
  });

  it("counts what is waiting in front of the title", () => {
    expect(tabTitle("AWBRN", 1)).toBe("(1) AWBRN");
    expect(tabTitle("AWBRN", 4)).toBe("(4) AWBRN");
  });

  it("keeps a large count to two characters", () => {
    expect(tabTitle("AWBRN", 10)).toBe("(9+) AWBRN");
    expect(tabTitle("AWBRN", 250)).toBe("(9+) AWBRN");
  });
});

describe("countLabel", () => {
  it("draws the count itself up to the limit", () => {
    expect(countLabel(9)).toBe("9");
    expect(countLabel(10)).toBe("9+");
  });
});

describe("faviconDataUrl", () => {
  it("draws no badge when nothing is waiting", () => {
    const icon = decodeURIComponent(faviconDataUrl(0));
    expect(icon.startsWith("data:image/svg+xml,")).toBe(true);
    expect(icon).not.toContain("circle");
  });

  it("draws the count on the icon when something is", () => {
    const icon = decodeURIComponent(faviconDataUrl(3));
    expect(icon).toContain("circle");
    expect(icon).toContain(">3<");
  });

  it("shrinks the text so a two character count still fits", () => {
    const many = decodeURIComponent(faviconDataUrl(12));
    const few = decodeURIComponent(faviconDataUrl(2));
    expect(many).toContain(">9+<");
    expect(many).toContain('font-size="9"');
    expect(few).toContain('font-size="11"');
  });

  it("escapes to a url a link element can carry unquoted", () => {
    expect(faviconDataUrl(1)).not.toContain("<");
    expect(faviconDataUrl(1)).not.toContain('"');
  });
});

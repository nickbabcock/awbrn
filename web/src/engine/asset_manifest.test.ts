import { afterEach, describe, expect, it, vi } from "vitest";
import { createGameAssetConfig } from "./asset_manifest";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("createGameAssetConfig", () => {
  it("resolves the game assets against the given base URL", () => {
    const config = createGameAssetConfig("https://example.com/matches/12");

    expect(config.staticAssetUrls.get("textures/tiles.png")).toMatch(
      /^https:\/\/example\.com\/\S+\.png$/,
    );
    for (const url of config.staticAssetUrls.values()) {
      expect(url.startsWith("https://example.com/")).toBe(true);
    }
  });

  it("explains that it needs the browser when there is no page URL", () => {
    vi.stubGlobal("location", undefined);

    expect(() => createGameAssetConfig()).toThrow(/must be created in the browser/);
  });
});

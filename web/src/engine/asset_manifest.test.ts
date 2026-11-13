import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("asset_manifest", () => {
  // The server bundle has no usable import.meta.url, so resolving an asset URL
  // against it while the module loads fails every server-rendered page.
  it("resolves no asset URLs when imported", async () => {
    vi.resetModules();
    const resolved: string[] = [];
    const OriginalURL = globalThis.URL;
    vi.stubGlobal(
      "URL",
      class extends OriginalURL {
        constructor(url: string | URL, base?: string | URL) {
          if (base !== undefined) resolved.push(String(url));
          super(url, base);
        }
      },
    );

    const manifest = await import("./asset_manifest");

    expect(resolved).toEqual([]);
    expect(manifest.coPortraitSheetAssetUrl).toEqual(expect.any(String));
    expect(manifest.coPortraitAtlas.portraits.length).toBeGreaterThan(0);
  });

  it("resolves the game assets against the given base URL", async () => {
    const { createGameAssetConfig } = await import("./asset_manifest");

    const config = createGameAssetConfig("https://example.com/assets/game_runner.js");

    expect([...config.staticAssetUrls.keys()]).toEqual([
      "textures/tiles.png",
      "textures/units.png",
      "textures/ui.png",
      "data/ui_atlas.json",
    ]);
    for (const url of config.staticAssetUrls.values()) {
      expect(url).toMatch(/^https:\/\/example\.com\/\S+\.(png|json)$/);
    }
  });
});

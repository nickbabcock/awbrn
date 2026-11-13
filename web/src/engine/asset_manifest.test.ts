import { describe, expect, it } from "vitest";
import { gameAssetPaths, resolveGameAssets } from "./asset_manifest";

describe("gameAssetPaths", () => {
  it("lists the engine assets at root-relative paths", () => {
    expect(gameAssetPaths.get("textures/tiles.png")).toMatch(/^\/\S+\.png$/);
  });
});

describe("resolveGameAssets", () => {
  it("resolves each path against the base URL", () => {
    const config = resolveGameAssets(
      new Map([["textures/tiles.png", "/assets/tiles-abc.png"]]),
      "https://cdn.example.com/assets/worker-123.js",
    );

    expect(config.staticAssetUrls).toEqual(
      new Map([["textures/tiles.png", "https://cdn.example.com/assets/tiles-abc.png"]]),
    );
  });
});

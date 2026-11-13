import { describe, expect, it } from "vitest";
import { gameAssetPaths } from "./asset_manifest";
import { resolveGameAssets } from "./resolve_game_assets";

describe("gameAssetPaths", () => {
  it("lists the engine assets at root-relative paths", () => {
    const path = gameAssetPaths.get("textures/tiles.png");
    expect(path?.startsWith("/")).toBe(true);
    expect(path?.endsWith(".png")).toBe(true);
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

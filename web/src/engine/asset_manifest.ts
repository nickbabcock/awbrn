import type { GameAssetConfig } from "#/wasm/awbrn_wasm.js";
import coPortraitAtlasData from "../../../assets/data/co_portraits.json";
import uiAtlasUrl from "../../../assets/data/ui_atlas.json?url";
import coPortraitSheetUrl from "../../../assets/textures/co_portraits.png?url";
import tilesTextureUrl from "../../../assets/textures/tiles.png?url";
import uiTextureUrl from "../../../assets/textures/ui.png?url";
import unitsTextureUrl from "../../../assets/textures/units.png?url";

const staticAssets: [key: string, url: string][] = [
  ["textures/tiles.png", tilesTextureUrl],
  ["textures/units.png", unitsTextureUrl],
  ["textures/ui.png", uiTextureUrl],
  ["data/ui_atlas.json", uiAtlasUrl],
];

/**
 * The engine's assets, with each URL made absolute against `baseUrl`.
 *
 * Call it from the browser only. The server bundle has no usable
 * import.meta.url, so resolving these when the module loads would fail every
 * server-rendered page that imports it.
 */
export function createGameAssetConfig(baseUrl: string | URL = import.meta.url): GameAssetConfig {
  return {
    staticAssetUrls: new Map(
      staticAssets.map(([key, url]) => [key, new URL(url, baseUrl).toString()]),
    ),
  };
}

export const coPortraitAtlas = coPortraitAtlasData;
export const coPortraitSheetAssetUrl = coPortraitSheetUrl;

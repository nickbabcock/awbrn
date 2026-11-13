import type { GameAssetConfig } from "#/wasm/awbrn_wasm.js";
import uiAtlasUrl from "../../../assets/data/ui_atlas.json?url";
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
 * The URLs Vite gives these assets are root-relative, and the engine's worker
 * needs absolute ones, so they are resolved against the page. That makes this
 * browser-only: the server has no page to resolve against, which is also why
 * nothing here is resolved when the module loads.
 */
export function createGameAssetConfig(
  baseUrl: string | URL | undefined = globalThis.location?.href,
): GameAssetConfig {
  if (baseUrl === undefined) {
    throw new Error(
      "Game asset URLs need the page URL to resolve against, so they must be created in the browser",
    );
  }

  return {
    staticAssetUrls: new Map(
      staticAssets.map(([key, url]) => [key, new URL(url, baseUrl).toString()]),
    ),
  };
}

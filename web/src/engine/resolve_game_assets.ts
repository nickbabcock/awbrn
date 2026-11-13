import type { GameAssetConfig } from "#/wasm/awbrn_wasm.js";

/**
 * The engine picks how to fetch an asset by its URL's scheme, so each path is
 * made absolute against `baseUrl`.
 */
export function resolveGameAssets(
  assetPaths: ReadonlyMap<string, string>,
  baseUrl: string | URL,
): GameAssetConfig {
  return {
    staticAssetUrls: new Map(
      Array.from(assetPaths, ([key, path]) => [key, new URL(path, baseUrl).toString()]),
    ),
  };
}

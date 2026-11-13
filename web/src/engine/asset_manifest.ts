import uiAtlasUrl from "../../../assets/data/ui_atlas.json?url";
import tilesTextureUrl from "../../../assets/textures/tiles.png?url";
import uiTextureUrl from "../../../assets/textures/ui.png?url";
import unitsTextureUrl from "../../../assets/textures/units.png?url";

/**
 * The engine's assets, keyed by the path the engine asks for, at the
 * root-relative URLs Vite gives them. The worker makes them absolute with
 * `resolveGameAssets` before the engine sees them.
 */
export const gameAssetPaths: ReadonlyMap<string, string> = new Map([
  ["textures/tiles.png", tilesTextureUrl],
  ["textures/units.png", unitsTextureUrl],
  ["textures/ui.png", uiTextureUrl],
  ["data/ui_atlas.json", uiAtlasUrl],
]);

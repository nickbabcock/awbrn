import type { GameAssetConfig } from "./wasm/awbrn_wasm";
import coPortraitAtlasData from "../../assets/data/co_portraits.json";
import uiAtlasUrl from "../../assets/data/ui_atlas.json?url";
import coPortraitSheetUrl from "../../assets/textures/co_portraits.png?url";
import tilesTextureUrl from "../../assets/textures/tiles.png?url";
import uiTextureUrl from "../../assets/textures/ui.png?url";
import unitsTextureUrl from "../../assets/textures/units.png?url";

const toAbsoluteAssetUrl = (assetUrl: string) => new URL(assetUrl, import.meta.url).toString();

export const staticAssetUrls = new Map<string, string>([
  ["textures/tiles.png", toAbsoluteAssetUrl(tilesTextureUrl)],
  ["textures/units.png", toAbsoluteAssetUrl(unitsTextureUrl)],
  ["textures/ui.png", toAbsoluteAssetUrl(uiTextureUrl)],
  ["data/ui_atlas.json", toAbsoluteAssetUrl(uiAtlasUrl)],
]);

export const gameAssetConfig: GameAssetConfig = {
  staticAssetUrls,
};

export const coPortraitAtlas = coPortraitAtlasData;
export const coPortraitSheetAssetUrl = toAbsoluteAssetUrl(coPortraitSheetUrl);

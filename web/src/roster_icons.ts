import type { CSSProperties } from "react";
import unitAtlasManifest from "../../assets/data/unit_atlas_manifest.json";
import uiAtlasData from "../../assets/data/ui_atlas.json";
import uiTextureUrl from "../../assets/textures/ui.png?url";
import unitsTextureUrl from "../../assets/textures/units.png?url";
import { factions } from "./factions";

const INFANTRY_VISIBLE_X = 7;
const INFANTRY_VISIBLE_Y = 8;
const INFANTRY_VISIBLE_WIDTH = 16;
const INFANTRY_VISIBLE_HEIGHT = 16;

type AtlasSprite = {
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
};

const UI_SPRITES = new Map(
  (uiAtlasData.sprites as AtlasSprite[]).map((sprite) => [sprite.name, sprite]),
);
const FACTION_ID_BY_CODE = new Map(factions.map((faction) => [faction.code, faction.id]));
const FACTION_INDEX = new Map(
  unitAtlasManifest.factions.map((faction) => [faction.id, faction.index]),
);
const UNIT_BASE_OFFSET = new Map(
  unitAtlasManifest.units.map((unit) => [unit.key, unit.baseOffset]),
);

export function getUiAtlasSprite(name: string): AtlasSprite | null {
  return UI_SPRITES.get(name) ?? null;
}

export function uiAtlasSpriteStyle(name: string): CSSProperties | null {
  const sprite = getUiAtlasSprite(name);
  if (!sprite) {
    return null;
  }

  return {
    width: `${sprite.width}px`,
    height: `${sprite.height}px`,
    backgroundImage: `url(${uiTextureUrl})`,
    backgroundPosition: `-${sprite.x}px -${sprite.y}px`,
    backgroundRepeat: "no-repeat",
  };
}

export function infantrySpriteStyle(factionCode: string): CSSProperties | null {
  const factionId = FACTION_ID_BY_CODE.get(factionCode);
  const factionOffset = factionId === undefined ? undefined : FACTION_INDEX.get(factionId);
  const infantryBaseOffset = UNIT_BASE_OFFSET.get("Infantry");
  if (factionOffset === undefined || infantryBaseOffset === undefined) {
    return null;
  }

  const spriteIndex = factionOffset * unitAtlasManifest.framesPerFaction + infantryBaseOffset;
  const column = spriteIndex % unitAtlasManifest.sheet.columns;
  const row = Math.floor(spriteIndex / unitAtlasManifest.sheet.columns);
  const x =
    unitAtlasManifest.sheet.offsetX +
    column * (unitAtlasManifest.sheet.cellWidth + unitAtlasManifest.sheet.paddingX);
  const y =
    unitAtlasManifest.sheet.offsetY +
    row * (unitAtlasManifest.sheet.cellHeight + unitAtlasManifest.sheet.paddingY);

  return {
    width: `${INFANTRY_VISIBLE_WIDTH}px`,
    height: `${INFANTRY_VISIBLE_HEIGHT}px`,
    backgroundImage: `url(${unitsTextureUrl})`,
    backgroundPosition: `-${x + INFANTRY_VISIBLE_X}px -${y + INFANTRY_VISIBLE_Y}px`,
    backgroundRepeat: "no-repeat",
  };
}

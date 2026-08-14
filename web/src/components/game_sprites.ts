import type { CSSProperties } from "react";
import terrainAtlasManifest from "../../../assets/data/terrain_atlas_manifest.json";
import unitAtlasManifest from "../../../assets/data/unit_atlas_manifest.json";
import uiAtlasData from "../../../assets/data/ui_atlas.json";
import uiTextureUrl from "../../../assets/textures/ui.png?url";
import tilesTextureUrl from "../../../assets/textures/tiles.png?url";
import unitsTextureUrl from "../../../assets/textures/units.png?url";
import { factions } from "#/factions.ts";
import type { UnitKind } from "#/wasm/awbrn_wasm.js";

const INFANTRY_VISIBLE_X = 7;
const INFANTRY_VISIBLE_Y = 8;
const INFANTRY_VISIBLE_WIDTH = 16;
const INFANTRY_VISIBLE_HEIGHT = 16;

/**
 * The part of a 23x24 atlas cell a standing unit actually occupies.
 *
 * The cell is sized for animation, so a unit is anchored to its lower right and
 * the rest of the cell is empty. Every one of the 25 units in all 20 armies
 * fits this window, measured off the atlas rather than assumed. It is 19 tall
 * rather than 16 because Noir Eclipse and Umber Wilds draw their infantry and
 * mech three pixels higher in the cell than every other army does; cropping to
 * a square would take the tops of their heads off.
 */
const UNIT_VISIBLE_X = 7;
const UNIT_VISIBLE_Y = 5;
const UNIT_VISIBLE_WIDTH = 16;
const UNIT_VISIBLE_HEIGHT = 19;
const TERRAIN_SHEET = terrainAtlasManifest.sheet;

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
// The engine names a unit in kebab case ("md-tank") and the atlas names the
// same unit in the source game's own casing ("MdTank"). Folding both to one
// form keeps the two vocabularies from needing a hand-written table that has to
// be extended every time a unit is added.
const UNIT_BASE_OFFSET_BY_KIND = new Map(
  unitAtlasManifest.units.map((unit) => [unit.key.toLowerCase(), unit.baseOffset]),
);

/** The full sheet, in its own pixels. The generator trims no trailing gutter. */
const UNIT_SHEET_WIDTH =
  unitAtlasManifest.sheet.columns *
  (unitAtlasManifest.sheet.cellWidth + unitAtlasManifest.sheet.paddingX);
const UNIT_SHEET_HEIGHT =
  unitAtlasManifest.sheet.rows *
  (unitAtlasManifest.sheet.cellHeight + unitAtlasManifest.sheet.paddingY);

/** The width and height a unit sprite occupies once drawn at `scale`. */
export function unitSpriteSize(scale = 1): { width: number; height: number } {
  return { width: UNIT_VISIBLE_WIDTH * scale, height: UNIT_VISIBLE_HEIGHT * scale };
}

/** One terrain atlas cell, including the part that rises above its map tile. */
export function terrainSpriteStyle(index: number, scale = 1): CSSProperties {
  const column = index % TERRAIN_SHEET.columns;
  const row = Math.floor(index / TERRAIN_SHEET.columns);

  return {
    width: `${TERRAIN_SHEET.cellWidth * scale}px`,
    height: `${TERRAIN_SHEET.cellHeight * scale}px`,
    backgroundImage: `url(${tilesTextureUrl})`,
    backgroundSize: `${TERRAIN_SHEET.cellWidth * TERRAIN_SHEET.columns * scale}px ${TERRAIN_SHEET.cellHeight * TERRAIN_SHEET.rows * scale}px`,
    backgroundPosition: `-${column * TERRAIN_SHEET.cellWidth * scale}px -${row * TERRAIN_SHEET.cellHeight * scale}px`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  };
}

/**
 * The tile square of one terrain cell, without what rises above it.
 *
 * A terrain cell is 16 wide and 32 tall because the upper half holds the peak,
 * the roof or the tower that overhangs the tile behind it. Somewhere the whole
 * cell has to be drawn, or a mountain is a picture of grass. Somewhere else —
 * a control on a row beside other controls — only the square is wanted, and
 * the square alone still tells a city from a sea from a wood.
 */
export function terrainTileStyle(index: number, scale = 1): CSSProperties {
  const column = index % TERRAIN_SHEET.columns;
  const row = Math.floor(index / TERRAIN_SHEET.columns);
  const tile = TERRAIN_SHEET.cellHeight / 2;

  return {
    width: `${TERRAIN_SHEET.cellWidth * scale}px`,
    height: `${tile * scale}px`,
    backgroundImage: `url(${tilesTextureUrl})`,
    backgroundSize: `${TERRAIN_SHEET.cellWidth * TERRAIN_SHEET.columns * scale}px ${TERRAIN_SHEET.cellHeight * TERRAIN_SHEET.rows * scale}px`,
    backgroundPosition: `-${column * TERRAIN_SHEET.cellWidth * scale}px -${(row * TERRAIN_SHEET.cellHeight + tile) * scale}px`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  };
}

export function getUiAtlasSprite(name: string): AtlasSprite | null {
  return UI_SPRITES.get(name) ?? null;
}

/**
 * A sprite from the interface atlas, drawn at a whole-number multiple.
 *
 * Scaling through the background box keeps every cell edge on a whole pixel, so
 * the art stays crisp instead of being resampled.
 */
export function uiAtlasSpriteStyle(name: string, scale = 1): CSSProperties | null {
  const sprite = getUiAtlasSprite(name);
  if (!sprite) {
    return null;
  }

  return {
    width: `${sprite.width * scale}px`,
    height: `${sprite.height * scale}px`,
    backgroundImage: `url(${uiTextureUrl})`,
    backgroundSize: `${uiAtlasData.size.width * scale}px ${uiAtlasData.size.height * scale}px`,
    backgroundPosition: `-${sprite.x * scale}px -${sprite.y * scale}px`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  };
}

/**
 * The idle frame of one unit in one army's colors, cropped to the unit itself.
 */
export function unitSpriteStyle(
  unit: UnitKind,
  factionCode: string,
  scale = 1,
): CSSProperties | null {
  const baseOffset = UNIT_BASE_OFFSET_BY_KIND.get(unit.replaceAll("-", ""));
  if (baseOffset === undefined) {
    return null;
  }

  return unitCellStyle(baseOffset, factionCode, scale);
}

/** The roster's unit-count icon: one infantryman, cropped to the figure. */
export function infantrySpriteStyle(factionCode: string): CSSProperties | null {
  const factionOffset = factionFrameOffset(factionCode);
  const infantryBaseOffset = UNIT_BASE_OFFSET.get("Infantry");
  if (factionOffset === undefined || infantryBaseOffset === undefined) {
    return null;
  }

  const { x, y } = unitCellOrigin(factionOffset + infantryBaseOffset);

  return {
    width: `${INFANTRY_VISIBLE_WIDTH}px`,
    height: `${INFANTRY_VISIBLE_HEIGHT}px`,
    backgroundImage: `url(${unitsTextureUrl})`,
    backgroundPosition: `-${x + INFANTRY_VISIBLE_X}px -${y + INFANTRY_VISIBLE_Y}px`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  };
}

function factionFrameOffset(factionCode: string): number | undefined {
  const factionId = FACTION_ID_BY_CODE.get(factionCode);
  const factionIndex = factionId === undefined ? undefined : FACTION_INDEX.get(factionId);
  return factionIndex === undefined ? undefined : factionIndex * unitAtlasManifest.framesPerFaction;
}

function unitCellOrigin(frame: number): { x: number; y: number } {
  const sheet = unitAtlasManifest.sheet;
  const column = frame % sheet.columns;
  const row = Math.floor(frame / sheet.columns);

  return {
    x: sheet.offsetX + column * (sheet.cellWidth + sheet.paddingX),
    y: sheet.offsetY + row * (sheet.cellHeight + sheet.paddingY),
  };
}

function unitCellStyle(
  baseOffset: number,
  factionCode: string,
  scale: number,
): CSSProperties | null {
  const factionOffset = factionFrameOffset(factionCode);
  if (factionOffset === undefined) {
    return null;
  }

  const { x, y } = unitCellOrigin(factionOffset + baseOffset);
  const size = unitSpriteSize(scale);

  return {
    width: `${size.width}px`,
    height: `${size.height}px`,
    backgroundImage: `url(${unitsTextureUrl})`,
    backgroundSize: `${UNIT_SHEET_WIDTH * scale}px ${UNIT_SHEET_HEIGHT * scale}px`,
    backgroundPosition: `-${(x + UNIT_VISIBLE_X) * scale}px -${(y + UNIT_VISIBLE_Y) * scale}px`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  };
}

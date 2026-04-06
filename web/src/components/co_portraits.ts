import { coPortraitAtlas, coPortraitSheetAssetUrl } from "#/engine/asset_manifest.ts";

interface CoPortraitAtlasEntry {
  index: number;
  key: string;
  displayName: string;
  awbwId: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

interface CoPortraitAtlasData {
  size: {
    width: number;
    height: number;
  };
  cellWidth: number;
  cellHeight: number;
  columns: number;
  rows: number;
  portraits: CoPortraitAtlasEntry[];
}

export interface CoPortraitEntry extends CoPortraitAtlasEntry {
  sheetUrl: string;
}

export type CoPortraitCatalog = Map<string, CoPortraitEntry>;

export const DEFAULT_CO_PORTRAIT_KEY = "no-co";

let catalog: CoPortraitCatalog | undefined;
let catalogByAwbwId: Map<number, CoPortraitEntry> | undefined;

export function loadCoPortraitCatalog(): CoPortraitCatalog {
  if (!catalog) {
    const portraits = (coPortraitAtlas as CoPortraitAtlasData).portraits.map((portrait) => ({
      ...portrait,
      sheetUrl: coPortraitSheetAssetUrl,
    }));
    catalog = new Map(portraits.map((portrait) => [portrait.key, portrait]));
    catalogByAwbwId = new Map(portraits.map((portrait) => [portrait.awbwId, portrait]));
  }

  return catalog;
}

export function listCoPortraits(): CoPortraitEntry[] {
  return Array.from(loadCoPortraitCatalog().values()).sort(
    (left, right) => left.awbwId - right.awbwId,
  );
}

export function getCoPortrait(coKey: string | null | undefined): CoPortraitEntry | null {
  return resolveCoPortrait(loadCoPortraitCatalog(), coKey);
}

export function getCoPortraitByAwbwId(awbwId: number | null | undefined): CoPortraitEntry | null {
  if (awbwId === null || awbwId === undefined) {
    return getCoPortrait(DEFAULT_CO_PORTRAIT_KEY);
  }

  loadCoPortraitCatalog();
  return catalogByAwbwId?.get(awbwId) ?? getCoPortrait(DEFAULT_CO_PORTRAIT_KEY);
}

export function resolveCoPortrait(
  catalog: CoPortraitCatalog | null | undefined,
  coKey: string | null | undefined,
): CoPortraitEntry | null {
  const activeCatalog = catalog ?? loadCoPortraitCatalog();

  if (coKey) {
    return activeCatalog.get(coKey) ?? activeCatalog.get(DEFAULT_CO_PORTRAIT_KEY) ?? null;
  }

  return activeCatalog.get(DEFAULT_CO_PORTRAIT_KEY) ?? null;
}

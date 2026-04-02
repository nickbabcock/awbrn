import { coPortraitAtlas, coPortraitSheetAssetUrl } from "./asset_manifest";

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

let catalog: CoPortraitCatalog | undefined;

export function loadCoPortraitCatalog(): CoPortraitCatalog {
  if (!catalog) {
    catalog = new Map(
      (coPortraitAtlas as CoPortraitAtlasData).portraits.map((portrait) => [
        portrait.key,
        {
          ...portrait,
          sheetUrl: coPortraitSheetAssetUrl,
        },
      ]),
    );
  }

  return catalog;
}

export function listCoPortraits(): CoPortraitEntry[] {
  return Array.from(loadCoPortraitCatalog().values()).sort(
    (left, right) => left.awbwId - right.awbwId,
  );
}

export function getCoPortrait(coKey: string | null | undefined): CoPortraitEntry | null {
  if (!coKey) {
    return null;
  }

  return loadCoPortraitCatalog().get(coKey) ?? null;
}

export function resolveCoPortrait(
  catalog: CoPortraitCatalog | null,
  coKey: string | null | undefined,
): CoPortraitEntry | null {
  if (!catalog || !coKey) {
    return null;
  }

  return catalog.get(coKey) ?? null;
}

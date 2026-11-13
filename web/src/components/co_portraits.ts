import coPortraitSheetAssetUrl from "../../../assets/textures/co_portraits.png?url";
import { coPortraitAtlas, DEFAULT_CO_PORTRAIT_KEY } from "#/co_roster.ts";

export { DEFAULT_CO_PORTRAIT_KEY };

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

export interface CoPortraitEntry extends CoPortraitAtlasEntry {
  sheetUrl: string;
  sheetWidth: number;
  sheetHeight: number;
}

export type CoPortraitCatalog = Map<string, CoPortraitEntry>;

let catalog: CoPortraitCatalog | undefined;
let catalogByAwbwId: Map<number, CoPortraitEntry> | undefined;

export function loadCoPortraitCatalog(): CoPortraitCatalog {
  if (!catalog) {
    const portraits = coPortraitAtlas.portraits.map((portrait) => ({
      ...portrait,
      sheetUrl: coPortraitSheetAssetUrl,
      sheetWidth: coPortraitAtlas.size.width,
      sheetHeight: coPortraitAtlas.size.height,
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

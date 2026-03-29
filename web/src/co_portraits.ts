const CO_PORTRAIT_DATA_URL = "/assets/data/co_portraits.json";
const CO_PORTRAIT_SHEET_URL = "/assets/textures/co_portraits.png";

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

let catalogPromise: Promise<CoPortraitCatalog> | undefined;

export async function loadCoPortraitCatalog(): Promise<CoPortraitCatalog> {
  if (!catalogPromise) {
    catalogPromise = fetch(CO_PORTRAIT_DATA_URL)
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`Failed to load CO portraits: ${response.status}`);
        }
        const data = (await response.json()) as CoPortraitAtlasData;
        return new Map(
          data.portraits.map((portrait) => [
            portrait.key,
            {
              ...portrait,
              sheetUrl: CO_PORTRAIT_SHEET_URL,
            },
          ]),
        );
      })
      .catch((error) => {
        catalogPromise = undefined;
        throw error;
      });
  }

  return catalogPromise;
}

export async function getCoPortrait(
  coKey: string | null | undefined,
): Promise<CoPortraitEntry | null> {
  if (!coKey) {
    return null;
  }

  const catalog = await loadCoPortraitCatalog();
  return catalog.get(coKey) ?? null;
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

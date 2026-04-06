import factionsData from "../../assets/data/factions.json";

export interface FactionCatalogEntry {
  id: number;
  code: string;
  displayName: string;
  facesRight: boolean;
}

export const factions: readonly FactionCatalogEntry[] = factionsData.factions;

const factionById = new Map(factions.map((faction) => [faction.id, faction]));
const factionByCode = new Map(factions.map((faction) => [faction.code, faction]));

export function getFactionById(factionId: number): FactionCatalogEntry | null {
  return factionById.get(factionId) ?? null;
}

export function getFactionByCode(factionCode: string): FactionCatalogEntry | null {
  return factionByCode.get(factionCode) ?? null;
}

export function defaultFactionIdForSlot(slotIndex: number): number {
  return factions[slotIndex % factions.length]?.id ?? factions[0]?.id ?? 1;
}

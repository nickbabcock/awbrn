import factionsData from "../../assets/data/factions.json";
import type { AwbwMapData } from "#/awbw/schemas.ts";

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

/**
 * The faction which owns each AWBW property terrain id.
 *
 * Generated from `AwbwTerrain` by `cargo xtask-assets factions`, so it stays in
 * step with the terrain ids the server accepts.
 */
const propertyOwnerByTerrainId = new Map(
  Object.entries(factionsData.propertyOwners).map(([terrainId, factionId]) => [
    Number(terrainId),
    factionId,
  ]),
);

/**
 * Return the faction each of a match's player slots starts with.
 *
 * AWBW maps keep their original army colors in their property terrain ids, so a
 * two-player map is not necessarily Orange Star against Blue Moon. Slot order
 * follows AWBW's faction order, which is also the catalog order.
 *
 * A slot faction decides which map properties the seat owns, so no two slots can
 * share one. A map which names fewer factions than it has seats takes the
 * remainder from the catalog. The faction a player later selects is a depiction
 * only and does not change these.
 */
export function mapSlotFactionIds(map: AwbwMapData, slotCount: number): number[] {
  const presentFactionIds = new Set<number>();
  for (const column of map["Terrain Map"]) {
    for (const terrainId of column) {
      const factionId = propertyOwnerByTerrainId.get(terrainId);
      if (factionId !== undefined) presentFactionIds.add(factionId);
    }
  }
  for (const unit of map["Predeployed Units"]) {
    const faction = getFactionByCode(unit["Country Code"]);
    if (faction) presentFactionIds.add(faction.id);
  }

  const slotFactionIds = factions
    .filter((faction) => presentFactionIds.has(faction.id))
    .slice(0, slotCount)
    .map((faction) => faction.id);

  const taken = new Set(slotFactionIds);
  for (const faction of factions) {
    if (slotFactionIds.length >= slotCount) break;
    if (taken.has(faction.id)) continue;
    taken.add(faction.id);
    slotFactionIds.push(faction.id);
  }

  return slotFactionIds;
}

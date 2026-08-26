import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { storeAwbwMap } from "./maps.server.ts";

/** The AWBW maps a development database starts with. */
const DEV_SEED_MAPS: Record<number, () => Promise<{ default: unknown }>> = {
  // Amber Valley, a 20x20 two player map.
  61748: () => import("../../../assets/maps/61748.json"),
};

let seeded: Promise<void> | null = null;

/**
 * Put the seed maps in the catalog, once for each server that starts.
 *
 * A local database starts empty, and a catalog with no maps makes a new match
 * impossible until someone imports a map by hand. The maps come from files in
 * the repository, so the seed needs no network and always gives the same
 * catalog. A map that is already held is left alone.
 *
 * A failed seed is not remembered: the next request tries again.
 */
export function seedDevMaps(): Promise<void> {
  seeded ??= runSeed().catch((error: unknown) => {
    seeded = null;
    console.error("[dev-seed] could not seed the map catalog", error);
  });
  return seeded;
}

async function runSeed(): Promise<void> {
  for (const [sourceMapId, load] of Object.entries(DEV_SEED_MAPS)) {
    const data = awbwMapDataSchema.parse((await load()).default);
    const { mapId } = await storeAwbwMap(Number(sourceMapId), data);
    console.log(`[dev-seed] map ${sourceMapId} is in the catalog as ${mapId}`);
  }
}

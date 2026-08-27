import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import { and, eq, isNull } from "drizzle-orm";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { maps, mapSources } from "#/db/global.ts";
import { storeAwbwMap } from "./maps.server.ts";

/**
 * The AWBW maps a development database starts with.
 *
 * The catalog is a board somebody browses, so a local one holds enough maps,
 * at enough sizes and player counts, to see a board rather than one plate.
 * Every file here is in the repository, so the seed needs no network.
 */
const DEV_SEED_MAPS: Record<number, () => Promise<{ default: unknown }>> = {
  // Amber Valley, a 20x20 two player map.
  61748: () => import("../../../assets/maps/61748.json"),
  // 1vs1 Normandie, 19x19, two players.
  67073: () => import("../../../assets/maps/67073.json"),
  // Beach Nation Vacation, 23x19, two players.
  73021: () => import("../../../assets/maps/73021.json"),
  // Redemption World, 21x19, two players.
  96502: () => import("../../../assets/maps/96502.json"),
  // Stormbringer, 27x27, six players.
  108806: () => import("../../../assets/maps/108806.json"),
  // Remnants Betwixt, 22x18, two players.
  146471: () => import("../../../assets/maps/146471.json"),
  // Foreign Invasion, 27x21, five players.
  162795: () => import("../../../assets/maps/162795.json"),
  // ! 2v2v1 Broken Isles, 37x37, five players.
  168602: () => import("../../../assets/maps/168602.json"),
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

/**
 * The seed maps somebody here wrote, and the account that wrote each one.
 *
 * An imported map has no author on this site, so `mapTagGrant` never reaches
 * its owner branch and `mapRankGrant` never refuses anybody. Both are rules
 * worth being able to see, and two attributed maps is what makes every
 * combination of them reachable by opening a map rather than by remembering
 * a step:
 *
 * - a map written by a player, where its author tags it with no reason asked
 *   and cannot rank it, because no player can;
 * - a map written by a moderator, where the same author holds the rank
 *   permission and is refused anyway, which is the rule the refusal exists
 *   for;
 * - every other map, which nobody here wrote, where a moderator tags with a
 *   reason and ranks freely.
 */
const DEV_AUTHORED_MAPS: Record<number, string> = {
  // Amber Valley, written by a plain player.
  61748: "player@awbrn.test",
  // 1vs1 Normandie, written by the moderator who may not grade it.
  67073: "moderator@awbrn.test",
};

/**
 * Say who here wrote which of the seed maps.
 *
 * `accounts` maps a seeded email to the id it holds, which is what
 * `seedDevAccounts` reports. A write only lands on a map that has no author,
 * so a change made in the application is not overwritten on the next start.
 */
export async function attributeDevMaps(accounts: ReadonlyMap<string, string>): Promise<void> {
  const db = drizzle(env.DB, { schema: { maps, mapSources } });

  for (const [sourceMapId, email] of Object.entries(DEV_AUTHORED_MAPS)) {
    const authorUserId = accounts.get(email);
    if (!authorUserId) continue;

    const source = await db
      .select({ mapId: mapSources.mapId })
      .from(mapSources)
      .where(and(eq(mapSources.source, "awbw"), eq(mapSources.sourceMapId, Number(sourceMapId))))
      .get();
    if (!source) continue;

    const written = await db
      .update(maps)
      .set({ authorUserId })
      .where(and(eq(maps.id, source.mapId), isNull(maps.authorUserId)))
      .run();
    if (written.meta.changes > 0) {
      console.log(`[dev-seed] map ${sourceMapId} is now authored by ${email}`);
    }
  }
}

import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { drizzle } from "drizzle-orm/d1";
import { mapRevisions, maps, user } from "#/db/global.ts";
import { defaultMatchClock, matchCreateRequestSchema } from "./schemas.ts";
import { createMatch } from "./matches.server.ts";
import { canonicalizeAwbwMap } from "#/server_wasm.ts";
import awbwMap from "../../../assets/maps/61748.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";

const db = drizzle(env.DB);
const creator = { id: "create-match-test-user", name: "Create Match Test" };

const request = {
  name: "Create Match Test",
  isPrivate: false,
  settings: {
    fogEnabled: false,
    startingFunds: 1000,
    hotseatEnabled: false,
    bannedCoIds: [],
    clock: defaultMatchClock,
  },
} as const;

let mapRef: { mapId: string; revision: number };

async function seed(): Promise<void> {
  await db
    .insert(user)
    .values({
      id: creator.id,
      name: creator.name,
      email: "create-match-test@example.com",
      emailVerified: true,
      updatedAt: new Date(),
    })
    .onConflictDoNothing();
  const imported = canonicalizeAwbwMap(awbwMapDataSchema.parse(awbwMap));
  const mapId = "cmtestmap001";
  const now = new Date();
  await env.CONTENT.put(
    `maps/doc/v1/${imported.contentHash}`,
    JSON.stringify({
      width: imported.document.width,
      height: imported.document.height,
      terrain: imported.document.terrain,
      units: imported.document.units,
    }),
  );
  await db
    .insert(maps)
    .values({
      id: mapId,
      name: imported.document.metadata.name,
      author: imported.document.metadata.author,
      currentRevision: 1,
      createdAt: now,
      updatedAt: now,
    })
    .onConflictDoNothing();
  await db
    .insert(mapRevisions)
    .values({
      mapId,
      revision: 1,
      contentHash: imported.contentHash,
      width: imported.document.width,
      height: imported.document.height,
      playerCount: imported.document.metadata.player_count,
      propertySignature: imported.propertySignature,
      unitSignature: imported.unitSignature,
      createdAt: now,
      lastSeenAt: now,
    })
    .onConflictDoNothing();
  mapRef = { mapId, revision: 1 };
}

describe("createMatch", () => {
  it("creates an open lobby", async () => {
    await seed();

    const result = await createMatch(
      matchCreateRequestSchema.parse({ ...request, map: mapRef }),
      creator,
    );

    expect(result.ok).toBe(true);
  });

  it("creates a lobby with an AI seat", async () => {
    await seed();

    const result = await createMatch(
      matchCreateRequestSchema.parse({
        ...request,
        map: mapRef,
        aiSeats: [{ slotIndex: 1, profileId: "ai-standard-v1" }],
      }),
      creator,
    );

    expect(result.ok).toBe(true);
  });

  it("rejects a session that names no account", async () => {
    await seed();

    const result = await createMatch(matchCreateRequestSchema.parse({ ...request, map: mapRef }), {
      id: "missing-create-match-user",
      name: "Missing User",
    });

    expect(result).toEqual({
      ok: false,
      error: {
        code: "notAuthenticated",
        message: "your account is not available; sign in again",
        httpStatus: 401,
        details: undefined,
      },
    });
  });
});

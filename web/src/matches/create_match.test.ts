import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { drizzle } from "drizzle-orm/d1";
import { eq } from "drizzle-orm";
import { matches, matchParticipants, mapRevisions, maps, user } from "#/db/global.ts";
import { defaultMatchClock, matchCreateRequestSchema } from "./schemas.ts";
import { createMatch, mutateMatch } from "./matches.server.ts";
import { canonicalizeAwbwMap } from "#/server_wasm.ts";
import awbwMap from "../../../assets/maps/61748.json";
import foreignInvasion from "../../../assets/maps/162795.json";
import normandie from "../../../assets/maps/67073.json";
import { coRoster } from "#/co_roster.ts";
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
  mapRef = await seedMap(awbwMap, "cmtestmap001");
}

async function seedMap(
  source: unknown,
  mapId: string,
): Promise<{ mapId: string; revision: number }> {
  await db
    .insert(user)
    .values({
      id: creator.id,
      name: creator.name,
      nameKey: creator.id,
      email: "create-match-test@example.com",
      emailVerified: true,
      updatedAt: new Date(),
    })
    .onConflictDoNothing();
  const imported = canonicalizeAwbwMap(awbwMapDataSchema.parse(source));
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
  return { mapId, revision: 1 };
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
        aiSeats: [{ slotIndex: 1, profileId: "ai-medium-v2" }],
      }),
      creator,
    );

    expect(result.ok).toBe(true);
  });

  it("gives an AI seat the army the host's roster chose, and refuses an unknown one", async () => {
    await seed();
    const create = (factionId: number) =>
      createMatch(
        matchCreateRequestSchema.parse({
          ...request,
          map: mapRef,
          aiSeats: [{ slotIndex: 1, profileId: "ai-medium-v2", factionId }],
        }),
        creator,
      );

    const created = await create(4);
    expect(created.ok).toBe(true);
    const seat = await db
      .select({ factionId: matchParticipants.factionId })
      .from(matchParticipants)
      .where(eq(matchParticipants.matchId, created.ok ? created.value.matchId : ""))
      .get();
    expect(seat?.factionId).toBe(4);

    const unknown = await create(999);
    expect(unknown.ok ? null : unknown.error.code).toBe("participantInvalid");
  });

  it("refuses a map that seats fewer than two armies", async () => {
    const terrain = Array.from({ length: 5 }, (_, x) =>
      Array.from({ length: 5 }, (_, y) => (x === 0 && y === 0 ? 42 : 1)),
    );
    const lone = await seedMap(
      {
        Name: "Lone Army",
        Author: "Create Match Test",
        "Player Count": 1,
        "Published Date": "2026-01-01 00:00:00",
        "Size X": 5,
        "Size Y": 5,
        "Terrain Map": terrain,
        "Predeployed Units": [],
      },
      "cmtestmap010",
    );

    const result = await createMatch(
      matchCreateRequestSchema.parse({ ...request, map: lone }),
      creator,
    );

    expect(result.ok ? null : result.error.code).toBe("invalidMap");
  });

  it("refuses a map where an army has no HQ, Lab or unit to lose", async () => {
    // An Orange Star HQ (42) and a Blue Moon city (43).
    const terrain = Array.from({ length: 5 }, (_, x) =>
      Array.from({ length: 5 }, (_, y) => (y === 0 ? (x === 0 ? 42 : x === 4 ? 43 : 1) : 1)),
    );
    const holdout = await seedMap(
      {
        Name: "Holdout",
        Author: "Create Match Test",
        "Player Count": 2,
        "Published Date": "2026-01-01 00:00:00",
        "Size X": 5,
        "Size Y": 5,
        "Terrain Map": terrain,
        "Predeployed Units": [],
      },
      "cmtestmap011",
    );

    const result = await createMatch(
      matchCreateRequestSchema.parse({ ...request, map: holdout }),
      creator,
    );

    expect(result.ok ? null : result.error.code).toBe("invalidMap");
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

  it("keeps two seats from holding the same army", async () => {
    await seed();
    const created = await createMatch(
      matchCreateRequestSchema.parse({
        ...request,
        map: mapRef,
        aiSeats: [{ slotIndex: 1, profileId: "ai-medium-v2" }],
      }),
      creator,
    );
    if (!created.ok) throw new Error(created.error.message);
    const { matchId } = created.value;
    const aiSeat = await db
      .select({ factionId: matchParticipants.factionId })
      .from(matchParticipants)
      .where(eq(matchParticipants.matchId, matchId))
      .get();
    const aiArmy = aiSeat!.factionId;
    const freeArmy = aiArmy === 1 ? 2 : 1;

    const joinTaken = await mutateMatch(matchId, creator, {
      action: "join",
      slotIndex: 0,
      factionId: aiArmy,
    });
    expect(joinTaken.ok ? null : joinTaken.error.code).toBe("armyTaken");

    const joined = await mutateMatch(matchId, creator, {
      action: "join",
      slotIndex: 0,
      factionId: freeArmy,
    });
    expect(joined.ok).toBe(true);

    const switched = await mutateMatch(matchId, creator, {
      action: "updateParticipant",
      slotIndex: 0,
      factionId: aiArmy,
    });
    expect(switched.ok ? null : switched.error.code).toBe("participantInvalid");
  });
});

describe("teams and per-seat bans", () => {
  const guest = { id: "create-match-test-guest", name: "Guest" };
  const [first, second, third] = coRoster.map((co) => co.awbwId) as [number, number, number];

  /** Foreign Invasion as a 4v1: seats 1 to 4 on A, a CPU in seat 4, the host alone on B. */
  async function fourVersusOne(seatBannedCoIds?: number[][]) {
    await seed();
    await db
      .insert(user)
      .values({
        ...guest,
        nameKey: guest.id,
        email: "guest@example.com",
        emailVerified: true,
        updatedAt: new Date(),
      })
      .onConflictDoNothing();
    const map = await seedMap(foreignInvasion, "cmtestmap005");
    const created = await createMatch(
      matchCreateRequestSchema.parse({
        ...request,
        map,
        settings: { ...request.settings, teams: [1, 1, 1, 1, 2], seatBannedCoIds },
        aiSeats: [{ slotIndex: 3, profileId: "ai-hard-v5" }],
      }),
      creator,
    );
    if (!created.ok) throw new Error(created.error.message);
    return created.value.matchId;
  }

  async function settingsOf(matchId: string) {
    return (await db.select().from(matches).where(eq(matches.id, matchId)).get())!.settings;
  }

  async function seat(matchId: string, slotIndex: number) {
    return db
      .select()
      .from(matchParticipants)
      .where(eq(matchParticipants.matchId, matchId))
      .all()
      .then((seats) => seats.find((entry) => entry.slotIndex === slotIndex)!);
  }

  it("stores a CO every seat bans in All when the match is made", async () => {
    const matchId = await fourVersusOne([[first, second], [second], [second], [second], [second]]);
    const settings = await settingsOf(matchId);
    expect(settings.bannedCoIds).toEqual([second]);
    expect(settings.seatBannedCoIds).toEqual([[first], [], [], [], []]);
    expect(settings.teams).toEqual([1, 1, 1, 1, 2]);
  });

  it("refuses teams that leave a single side", async () => {
    await seed();
    const map = await seedMap(foreignInvasion, "cmtestmap005");
    const created = await createMatch(
      matchCreateRequestSchema.parse({
        ...request,
        map,
        settings: { ...request.settings, teams: [1, 1, 1, 1, 1] },
      }),
      creator,
    );
    expect(created.ok ? null : created.error.code).toBe("participantInvalid");
  });

  it("refuses a seat whose own bans leave it no CO", async () => {
    await seed();
    const map = await seedMap(foreignInvasion, "cmtestmap005");
    const created = await createMatch(
      matchCreateRequestSchema.parse({
        ...request,
        map,
        settings: {
          ...request.settings,
          seatBannedCoIds: [coRoster.map((co) => co.awbwId), [], [], [], []],
        },
      }),
      creator,
    );
    expect(created.ok ? null : created.error.code).toBe("participantInvalid");
  });

  it("keeps a seat from picking a CO its own list bans", async () => {
    const matchId = await fourVersusOne([[first], [], [], [], []]);
    await mutateMatch(matchId, guest, { action: "join", slotIndex: 0, factionId: 1 });
    const picked = await mutateMatch(matchId, guest, {
      action: "updateParticipant",
      slotIndex: 0,
      coId: first,
    });
    expect(picked.ok ? null : picked.error.code).toBe("participantInvalid");
  });

  it("lets only the host move a seat, and clears every human Ready but a CPU's", async () => {
    const matchId = await fourVersusOne();
    await mutateMatch(matchId, guest, { action: "join", slotIndex: 0, factionId: 1 });
    await mutateMatch(matchId, guest, { action: "updateParticipant", slotIndex: 0, coId: third });
    await mutateMatch(matchId, guest, { action: "updateParticipant", slotIndex: 0, ready: true });
    expect((await seat(matchId, 0)).ready).toBe(true);

    const byGuest = await mutateMatch(matchId, guest, { action: "setTeam", slotIndex: 3, team: 2 });
    expect(byGuest.ok ? null : byGuest.error.code).toBe("notHost");

    const moved = await mutateMatch(matchId, creator, { action: "setTeam", slotIndex: 3, team: 2 });
    expect(moved.ok).toBe(true);
    expect((await settingsOf(matchId)).teams).toEqual([1, 1, 1, 2, 2]);
    expect((await seat(matchId, 0)).ready).toBe(false);
    expect((await seat(matchId, 3)).ready).toBe(true);
  });

  it("refuses ban edits from a guest and on a seat a person holds", async () => {
    const matchId = await fourVersusOne();
    await mutateMatch(matchId, guest, { action: "join", slotIndex: 0, factionId: 1 });

    const byGuest = await mutateMatch(matchId, guest, {
      action: "setSeatBans",
      slotIndex: 1,
      coIds: [first],
    });
    expect(byGuest.ok ? null : byGuest.error.code).toBe("notHost");

    const onPerson = await mutateMatch(matchId, creator, {
      action: "setSeatBans",
      slotIndex: 0,
      coIds: [first],
    });
    expect(onPerson.ok ? null : onPerson.error.code).toBe("participantInvalid");
  });

  it("has a CPU whose CO is banned pick again and stay ready", async () => {
    const matchId = await fourVersusOne();
    const before = await seat(matchId, 3);

    const banned = await mutateMatch(matchId, creator, {
      action: "setSeatBans",
      slotIndex: 3,
      coIds: [before.coId!],
    });
    expect(banned.ok).toBe(true);
    const after = await seat(matchId, 3);
    expect(after.coId).not.toBe(before.coId);
    expect(after.ready).toBe(true);
    expect((await settingsOf(matchId)).seatBannedCoIds?.[3]).toEqual([before.coId]);
  });
});

describe("unit settings", () => {
  const units = { unitBans: ["piperunner"], labUnits: ["neo-tank"] };

  it("refuses Lab units on a map with no Lab", async () => {
    await seed();
    const result = await createMatch(
      matchCreateRequestSchema.parse({
        ...request,
        map: mapRef,
        settings: { ...request.settings, ...units },
      }),
      creator,
    );
    expect(result).toMatchObject({
      ok: false,
      error: {
        code: "participantInvalid",
        message: "this map has no Lab, so it can have no Lab units",
      },
    });
  });

  it("keeps both lists on a map with a Lab", async () => {
    await seed();
    const map = await seedMap(normandie, "cmtestmap009");
    const result = await createMatch(
      matchCreateRequestSchema.parse({
        ...request,
        map,
        settings: { ...request.settings, ...units },
      }),
      creator,
    );
    if (!result.ok) throw new Error(result.error.message);
    const row = await db.select().from(matches).where(eq(matches.id, result.value.matchId)).get();
    expect(row!.settings).toMatchObject(units);
  });
});

/**
 * What a profile tells a visitor, against a real database: someone else
 * never reads a pairing awaiting confirmation or an invite-only match, and
 * the player reads both on their own profile. A ranked match is stored
 * private, yet anyone reads it.
 */

import { env } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import { listCompletedMatches } from "#/matches/matches.server.ts";
import { readPlayerProfile } from "./players.server.ts";

const AT = new Date("2026-08-28T18:00:00.000Z");
const seconds = (date: Date) => Math.floor(date.getTime() / 1000);

const SETTINGS = JSON.stringify({
  fogEnabled: false,
  startingFunds: 1000,
  hotseatEnabled: false,
  bannedCoIds: [],
  clock: { initialMs: 604_800_000, incrementMs: 172_800_000, maxBankMs: 604_800_000 },
});

function insertUser(id: string, name: string): D1PreparedStatement {
  return env.DB.prepare(
    "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
  ).bind(id, name, `${id}@example.com`, seconds(AT), seconds(AT));
}

function insertMatch(
  id: string,
  phase: string,
  isPrivate: boolean,
  pool: string | null = null,
): D1PreparedStatement {
  return env.DB.prepare(`
      INSERT INTO matches
        (id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers, isPrivate, settings, createdAt, updatedAt, completedAt, pool, season)
      VALUES (?, ?, ?, 'player', '000000061748', 1, 2, ?, ?, ?, ?, ?, ?, ?)
    `).bind(
    id,
    id,
    phase,
    isPrivate ? 1 : 0,
    SETTINGS,
    seconds(AT),
    seconds(AT),
    phase === "completed" ? seconds(AT) : null,
    pool,
    pool === null ? null : 1,
  );
}

function insertSeat(matchId: string, userId: string, slotIndex: number): D1PreparedStatement {
  return env.DB.prepare(`
      INSERT INTO match_participants (matchId, userId, slotIndex, factionId, coId, ready, joinedAt, updatedAt)
      VALUES (?, ?, ?, ?, 5, ?, ?, ?)
    `).bind(
    matchId,
    userId,
    slotIndex,
    slotIndex + 1,
    userId === "player" ? 0 : 1,
    seconds(AT),
    seconds(AT),
  );
}

function insertResult(matchId: string, userId: string, slotIndex: number, won: boolean) {
  return env.DB.prepare(`
      INSERT INTO match_results (matchId, slotIndex, userId, outcome, placement, reason, recordedAt)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `).bind(
    matchId,
    slotIndex,
    userId,
    won ? "win" : "loss",
    won ? 1 : 2,
    won ? null : "rout",
    seconds(AT),
  );
}

function finished(id: string, isPrivate: boolean, pool: string | null = null) {
  return [
    insertMatch(id, "completed", isPrivate, pool),
    insertSeat(id, "player", 0),
    insertSeat(id, "rival", 1),
    insertResult(id, "player", 0, true),
    insertResult(id, "rival", 1, false),
  ];
}

function ongoing(id: string, phase: string, isPrivate: boolean, pool: string | null = null) {
  return [
    insertMatch(id, phase, isPrivate, pool),
    insertSeat(id, "player", 0),
    insertSeat(id, "rival", 1),
  ];
}

beforeEach(async () => {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM match_voids"),
    env.DB.prepare("DELETE FROM match_results"),
    env.DB.prepare("DELETE FROM match_participants"),
    env.DB.prepare("DELETE FROM matches"),
    env.DB.prepare("DELETE FROM seasons"),
    env.DB.prepare("DELETE FROM map_revisions"),
    env.DB.prepare("DELETE FROM maps"),
    env.DB.prepare("DELETE FROM user"),
  ]);
  await env.DB.batch([
    insertUser("player", "Player"),
    insertUser("rival", "Rival"),
    env.DB.prepare("INSERT INTO seasons (number, startsAt, endsAt) VALUES (1, ?, ?)").bind(
      seconds(AT) - 86_400,
      seconds(AT) + 86_400,
    ),
    env.DB.prepare(
      "INSERT INTO maps (id, name, author, currentRevision, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
    ).bind("000000061748", "Amber Valley", "AWBW", seconds(AT), seconds(AT)),
    env.DB.prepare(`
        INSERT INTO map_revisions
          (mapId, revision, contentHash, width, height, playerCount, propertySignature, unitSignature, createdAt)
        VALUES ('000000061748', 1, 'hash', 20, 20, 2, 'p', 'u', ?)
      `).bind(seconds(AT)),
  ]);
  await env.DB.batch([
    ...ongoing("public-game", "active", false),
    ...ongoing("private-game", "active", true),
    ...ongoing("pairing", "pending", false),
    ...ongoing("lobby", "lobby", false),
    ...finished("public-done", false),
    ...finished("private-done", true),
    // Ranked matches are stored private, with no invite.
    ...ongoing("ranked-game", "active", true, "async"),
    ...finished("ranked-done", true, "async"),
    // The player on both sides, and a game voided by moderation: neither is a CO's record.
    insertMatch("hotseat-done", "completed", false),
    insertSeat("hotseat-done", "player", 0),
    insertSeat("hotseat-done", "player", 1),
    insertResult("hotseat-done", "player", 0, true),
    insertResult("hotseat-done", "player", 1, false),
    ...finished("voided-done", false),
    env.DB.prepare(
      "INSERT INTO match_voids (matchId, publicReason, voidedAt) VALUES ('voided-done', 'Cheating', ?)",
    ).bind(seconds(AT)),
  ]);
});

describe("readPlayerProfile", () => {
  it("is null for an account that does not exist", async () => {
    expect(await readPlayerProfile("nobody", null, AT)).toBeNull();
  });

  it("lists public and ranked games in progress to someone else", async () => {
    const profile = await readPlayerProfile("player", "rival", AT);
    expect(profile?.isViewer).toBe(false);
    expect(profile?.ongoing.map((match) => match.matchId).sort()).toEqual([
      "public-game",
      "ranked-game",
    ]);
    expect(profile?.ongoing[0]?.opponents[0]).toMatchObject({ userId: "rival", userName: "Rival" });
  });

  it("lists the pairing and the private game to the player, the pairing first and its opponent withheld", async () => {
    const profile = await readPlayerProfile("player", "player", AT);
    expect(profile?.ongoing.map((match) => match.matchId).sort()).toEqual(
      ["pairing", "private-game", "public-game", "ranked-game"].sort(),
    );
    const [lead] = profile!.ongoing;
    expect(lead).toMatchObject({ matchId: "pairing", isOwedPairing: true });
    expect(lead!.opponents[0]).toMatchObject({ userId: null, coId: null, isHidden: true });
    expect(JSON.stringify(lead)).not.toContain("Rival");
  });

  it("counts a CO's record from the matches the visitor may see, less hotseat and voided games", async () => {
    const theirs = await readPlayerProfile("player", null, AT);
    const own = await readPlayerProfile("player", "player", AT);
    expect(theirs?.cos).toEqual([{ coId: 5, factionId: 1, wins: 2, losses: 0, draws: 0 }]);
    expect(own?.cos).toEqual([{ coId: 5, factionId: 1, wins: 3, losses: 0, draws: 0 }]);
  });
});

describe("listCompletedMatches", () => {
  it("leaves an invite-only match out unless the player asks", async () => {
    const theirs = await listCompletedMatches("player", {}, { includePrivate: false });
    const own = await listCompletedMatches("player", {}, { includePrivate: true });
    if (!theirs.ok || !own.ok) throw new Error("history failed to load");
    const ids = (matches: readonly { matchId: string }[]) =>
      matches.map((match) => match.matchId).sort();
    expect(ids(theirs.value.matches)).toEqual([
      "hotseat-done",
      "public-done",
      "ranked-done",
      "voided-done",
    ]);
    expect(ids(own.value.matches)).toEqual([
      "hotseat-done",
      "private-done",
      "public-done",
      "ranked-done",
      "voided-done",
    ]);
    expect(theirs.value.matches.find((match) => match.matchId === "public-done")).toMatchObject({
      mapName: "Amber Valley",
      playerSlotIndexes: [0],
    });
  });
});

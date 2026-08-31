/**
 * The count the nav badge reads, against a real database.
 *
 * The badge is a promise that a player who is not looking at the match list
 * still learns their turn has come, so the checks here watch the three cases
 * it counts and the two ways the count is easy to overstate: a match waiting
 * on the opponent, and a hotseat match the viewer holds two seats in.
 */

import { env } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import { countMatchesAwaitingViewer } from "./matches.server.ts";

const OPENED_AT = new Date("2026-08-28T18:00:00.000Z");
const DEADLINE = new Date("2026-09-04T18:00:00.000Z");

function seconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

function insertUser(id: string, name: string): D1PreparedStatement {
  return env.DB.prepare(
    "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
  ).bind(id, name, `${id}@example.com`, seconds(OPENED_AT), seconds(OPENED_AT));
}

function insertMatch(
  id: string,
  phase: string,
  turn: { activeSlotIndex: number | null } = { activeSlotIndex: null },
): D1PreparedStatement {
  const settings = JSON.stringify({
    fogEnabled: false,
    startingFunds: 1000,
    hotseatEnabled: false,
    bannedCoIds: [],
    clock: { initialMs: 604_800_000, incrementMs: 172_800_000, maxBankMs: 604_800_000 },
  });
  return env.DB.prepare(`
      INSERT INTO matches
        (id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers, isPrivate, joinSlug, settings, createdAt, updatedAt, activeSlotIndex, turnDeadlineAt)
      VALUES (?, 'Match', ?, 'viewer', '000000061748', 1, 2, 0, NULL, ?, ?, ?, ?, ?)
    `).bind(
    id,
    phase,
    settings,
    seconds(OPENED_AT),
    seconds(OPENED_AT),
    turn.activeSlotIndex,
    turn.activeSlotIndex === null ? null : seconds(DEADLINE),
  );
}

function insertSeat(
  matchId: string,
  userId: string,
  slotIndex: number,
  options: { coId: number | null; ready: boolean },
): D1PreparedStatement {
  return env.DB.prepare(`
      INSERT INTO match_participants (matchId, userId, slotIndex, factionId, coId, ready, joinedAt, updatedAt)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `).bind(
    matchId,
    userId,
    slotIndex,
    slotIndex + 1,
    options.coId,
    options.ready ? 1 : 0,
    seconds(OPENED_AT),
    seconds(OPENED_AT),
  );
}

const READY = { coId: 5, ready: true };

async function awaiting(): Promise<number> {
  const result = await countMatchesAwaitingViewer("viewer");
  if (!result.ok) throw new Error(result.error.message);
  return result.value.awaiting;
}

beforeEach(async () => {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM match_participants"),
    env.DB.prepare("DELETE FROM matches"),
    env.DB.prepare("DELETE FROM map_revisions"),
    env.DB.prepare("DELETE FROM maps"),
    env.DB.prepare("DELETE FROM user"),
  ]);

  await env.DB.batch([
    insertUser("viewer", "Viewer"),
    insertUser("rival", "Rival"),
    env.DB.prepare(
      "INSERT INTO maps (id, name, author, currentRevision, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
    ).bind("000000061748", "Amber Valley", "AWBW", seconds(OPENED_AT), seconds(OPENED_AT)),
    env.DB.prepare(`
        INSERT INTO map_revisions
          (mapId, revision, contentHash, width, height, playerCount, propertySignature, unitSignature, createdAt)
        VALUES ('000000061748', 1, 'hash', 20, 20, 2, 'p', 'u', ?)
      `).bind(seconds(OPENED_AT)),
  ]);
});

describe("countMatchesAwaitingViewer", () => {
  it("counts nothing for a viewer with no matches", async () => {
    expect(await awaiting()).toBe(0);
  });

  it("counts the match whose turn the viewer holds, and not the opponent's", async () => {
    await env.DB.batch([
      insertMatch("mine", "active", { activeSlotIndex: 0 }),
      insertSeat("mine", "viewer", 0, READY),
      insertSeat("mine", "rival", 1, READY),
      insertMatch("theirs", "active", { activeSlotIndex: 1 }),
      insertSeat("theirs", "viewer", 0, READY),
      insertSeat("theirs", "rival", 1, READY),
    ]);

    expect(await awaiting()).toBe(1);
  });

  it("counts a hotseat match the viewer holds two seats in once", async () => {
    await env.DB.batch([
      insertMatch("hotseat", "active", { activeSlotIndex: 1 }),
      insertSeat("hotseat", "viewer", 0, READY),
      insertSeat("hotseat", "viewer", 1, READY),
    ]);

    expect(await awaiting()).toBe(1);
  });

  it("counts a ranked pairing the viewer has not confirmed", async () => {
    await env.DB.batch([
      insertMatch("pending", "pending"),
      insertSeat("pending", "viewer", 0, { coId: 5, ready: false }),
      insertSeat("pending", "rival", 1, READY),
      insertMatch("confirmed", "pending"),
      insertSeat("confirmed", "viewer", 0, READY),
      insertSeat("confirmed", "rival", 1, { coId: 5, ready: false }),
    ]);

    expect(await awaiting()).toBe(1);
  });

  it("counts a lobby seat that is unready or has no CO", async () => {
    await env.DB.batch([
      insertMatch("unready", "lobby"),
      insertSeat("unready", "viewer", 0, { coId: 5, ready: false }),
      insertMatch("no-co", "lobby"),
      insertSeat("no-co", "viewer", 0, { coId: null, ready: true }),
      insertMatch("set-up", "lobby"),
      insertSeat("set-up", "viewer", 0, READY),
    ]);

    expect(await awaiting()).toBe(2);
  });

  it("drops a match that has finished, whatever turn it last published", async () => {
    await env.DB.batch([
      insertMatch("done", "completed", { activeSlotIndex: 0 }),
      insertSeat("done", "viewer", 0, READY),
      insertMatch("starting", "starting", { activeSlotIndex: 0 }),
      insertSeat("starting", "viewer", 0, READY),
    ]);

    expect(await awaiting()).toBe(0);
  });
});

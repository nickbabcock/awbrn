import { describe, expect, it } from "vitest";
import { lobbyPollInterval, lobbySignature } from "./lobby_poll";
import type { MatchSnapshot } from "./schemas";

describe("lobbyPollInterval", () => {
  it("stays quick while the lobby is actively moving", () => {
    expect(lobbyPollInterval(0)).toBe(4_000);
    expect(lobbyPollInterval(45_000)).toBe(4_000);
  });

  it("steps down as the lobby goes quiet", () => {
    expect(lobbyPollInterval(46_000)).toBe(15_000);
    expect(lobbyPollInterval(5 * 60_000)).toBe(15_000);
    expect(lobbyPollInterval(5 * 60_000 + 1)).toBe(60_000);
    expect(lobbyPollInterval(30 * 60_000)).toBe(60_000);
  });

  it("holds at the floor for a wait measured in days", () => {
    const oneDay = 24 * 60 * 60_000;

    expect(lobbyPollInterval(31 * 60_000)).toBe(5 * 60_000);
    expect(lobbyPollInterval(oneDay)).toBe(5 * 60_000);
    // A page left open for three days must not have escalated its own cost.
    expect(lobbyPollInterval(3 * oneDay)).toBe(5 * 60_000);
  });
});

function snapshot(overrides: Partial<MatchSnapshot> = {}): MatchSnapshot {
  return {
    matchId: "m1",
    name: "Foreign Invasion",
    phase: "lobby",
    confirmationDeadlineAt: null,
    creatorUserId: "u1",
    creatorName: "Nick",
    mapId: "000000162795",
    mapRevision: 1,
    maxPlayers: 2,
    isPrivate: false,
    joinSlug: null,
    settings: { fogEnabled: false, startingFunds: 0 } as MatchSnapshot["settings"],
    createdAt: "2026-08-03T00:00:00.000Z",
    updatedAt: "2026-08-03T00:00:00.000Z",
    startedAt: null,
    completedAt: null,
    void: null,
    participants: [
      {
        userId: "u1",
        aiProfileId: null,
        userName: "Nick",
        slotIndex: 0,
        factionId: 1,
        coId: 5,
        ready: false,
        joinedAt: "2026-08-03T00:00:00.000Z",
        updatedAt: "2026-08-03T00:00:00.000Z",
      },
    ],
    ...overrides,
  };
}

describe("lobbySignature", () => {
  it("ignores churn a waiting player is not waiting on", () => {
    expect(lobbySignature(snapshot())).toBe(
      lobbySignature(snapshot({ updatedAt: "2026-08-04T00:00:00.000Z" })),
    );
  });

  it("changes when someone readies up", () => {
    const readied = snapshot({
      participants: [{ ...snapshot().participants[0]!, ready: true }],
    });

    expect(lobbySignature(readied)).not.toBe(lobbySignature(snapshot()));
  });

  it("changes when the match starts", () => {
    expect(lobbySignature(snapshot({ phase: "starting" }))).not.toBe(lobbySignature(snapshot()));
  });
});

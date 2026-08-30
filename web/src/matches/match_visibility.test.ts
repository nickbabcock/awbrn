import { describe, expect, it } from "vitest";
import { applyViewerVisibility } from "./match_visibility.ts";
import type { MatchParticipantSnapshot, MatchPhase, MatchSnapshot } from "./schemas.ts";

const VIEWER = "player-one";
const OPPONENT = "player-two";

function participant(
  userId: string,
  overrides: Partial<MatchParticipantSnapshot> = {},
): MatchParticipantSnapshot {
  return {
    userId,
    userName: userId === VIEWER ? "Andy" : "Sami",
    slotIndex: userId === VIEWER ? 0 : 1,
    factionId: 1,
    coId: 5,
    ready: false,
    joinedAt: "2026-08-28T18:00:00.000Z",
    updatedAt: "2026-08-28T18:00:00.000Z",
    ...overrides,
  };
}

function pairing(phase: MatchPhase, overrides: Partial<MatchSnapshot> = {}): MatchSnapshot {
  return {
    matchId: "m1",
    name: "Ranked async",
    phase,
    confirmationDeadlineAt: "2026-08-29T18:00:00.000Z",
    // The server makes the match for one of the two players, so the creator
    // fields hold a real name.
    creatorUserId: OPPONENT,
    creatorName: "Sami",
    mapId: "000000061748",
    mapRevision: 1,
    maxPlayers: 2,
    isPrivate: true,
    joinSlug: null,
    settings: {
      fogEnabled: false,
      startingFunds: 1000,
      hotseatEnabled: false,
      bannedCoIds: [],
      clock: { initialMs: 0, incrementMs: 0, maxBankMs: 0 },
    },
    createdAt: "2026-08-28T18:00:00.000Z",
    updatedAt: "2026-08-28T18:00:00.000Z",
    startedAt: null,
    completedAt: null,
    participants: [participant(VIEWER), participant(OPPONENT)],
    void: null,
    ...overrides,
  };
}

describe("a pending ranked pairing", () => {
  it("carries no trace of the other player", () => {
    const seen = applyViewerVisibility(pairing("pending"), VIEWER);
    const payload = JSON.stringify(seen);

    expect(payload).not.toContain("Sami");
    expect(payload).not.toContain(OPPONENT);
  });

  it("keeps the viewer's own seat whole", () => {
    const seen = applyViewerVisibility(pairing("pending"), VIEWER);
    const seat = seen.participants.find((entry) => entry.userId === VIEWER);

    expect(seat?.userName).toBe("Andy");
    expect(seat?.coId).toBe(5);
    expect(seat?.factionId).toBe(1);
  });

  it("hides the opponent's commander as well as their name", () => {
    const seen = applyViewerVisibility(pairing("pending"), VIEWER);
    const opponent = seen.participants.find((entry) => entry.userId !== VIEWER);

    expect(opponent?.coId).toBeNull();
    expect(opponent?.userName).toBe("Opponent");
  });

  it("still says whether the opponent is ready", () => {
    const snapshot = pairing("pending", {
      participants: [participant(VIEWER), participant(OPPONENT, { ready: true })],
    });
    const seen = applyViewerVisibility(snapshot, VIEWER);

    expect(seen.participants.find((entry) => entry.userId !== VIEWER)?.ready).toBe(true);
  });

  it("reveals both players once neither can refuse", () => {
    const snapshot = pairing("pending", {
      participants: [participant(VIEWER, { ready: true }), participant(OPPONENT, { ready: true })],
    });
    const seen = applyViewerVisibility(snapshot, VIEWER);

    expect(seen.participants.find((entry) => entry.userId === OPPONENT)?.userName).toBe("Sami");
    expect(seen.creatorName).toBe("Sami");
  });

  it("leaves an ordinary lobby alone", () => {
    const seen = applyViewerVisibility(pairing("lobby"), VIEWER);

    expect(seen.creatorName).toBe("Sami");
    expect(seen.participants.find((entry) => entry.userId === OPPONENT)?.userName).toBe("Sami");
  });
});

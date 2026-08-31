import { describe, expect, it } from "vitest";
import {
  formatMyMatchPhaseLabel,
  groupMyMatchRows,
  myMatchActionLabel,
  myMatchPhaseRank,
  needsViewerAction,
  ONGOING_MATCH_PHASES,
} from "./my_matches.ts";
import type { MatchPhase, MyMatchParticipantSummary, MyMatchSummary } from "./schemas.ts";

describe("my matches phases", () => {
  it("defines the ongoing match phases", () => {
    expect(ONGOING_MATCH_PHASES).toEqual(["draft", "lobby", "pending", "starting", "active"]);
  });

  it("orders active work before setup phases", () => {
    const phases: MatchPhase[] = ["draft", "lobby", "pending", "starting", "active"];
    expect(phases.sort((a, b) => myMatchPhaseRank(a) - myMatchPhaseRank(b))).toEqual([
      "active",
      "starting",
      "lobby",
      "pending",
      "draft",
    ]);
  });

  it("uses stable player-facing phase and action labels", () => {
    expect(formatMyMatchPhaseLabel("active")).toBe("Active");
    expect(myMatchActionLabel("active")).toBe("Open Match");
    expect(myMatchActionLabel("starting")).toBe("View Starting Match");
    expect(myMatchActionLabel("lobby")).toBe("Open Lobby");
    expect(myMatchActionLabel("pending")).toBe("Confirm Match");
  });
});

describe("my matches hotseat grouping", () => {
  it("keeps one match row with every owned seat", () => {
    expect(
      groupMyMatchRows([
        { matchId: "a", slotIndex: 0 },
        { matchId: "b", slotIndex: 1 },
        { matchId: "a", slotIndex: 2 },
      ]),
    ).toEqual([
      [
        { matchId: "a", slotIndex: 0 },
        { matchId: "a", slotIndex: 2 },
      ],
      [{ matchId: "b", slotIndex: 1 }],
    ]);
  });
});

function seat(slotIndex: number, seatOptions: Partial<MyMatchParticipantSummary> = {}) {
  return {
    slotIndex,
    factionId: slotIndex + 1,
    coId: 1,
    ready: true,
    joinedAt: "2026-08-28T18:00:00.000Z",
    updatedAt: "2026-08-28T18:00:00.000Z",
    ...seatOptions,
  } satisfies MyMatchParticipantSummary;
}

function summary(overrides: Partial<MyMatchSummary>): MyMatchSummary {
  return {
    matchId: "match",
    name: "Match",
    phase: "active",
    creatorName: "Host",
    mapId: "map",
    mapRevision: 1,
    maxPlayers: 2,
    participantCount: 2,
    openSlotCount: 0,
    isPrivate: false,
    settings: {
      fogEnabled: false,
      startingFunds: 1000,
      hotseatEnabled: false,
      bannedCoIds: [],
      clock: { initialMs: 1000, incrementMs: 0, maxBankMs: 1000 },
    },
    createdAt: "2026-08-28T18:00:00.000Z",
    updatedAt: "2026-08-28T18:00:00.000Z",
    startedAt: null,
    activeSlotIndex: null,
    turnDeadlineAt: null,
    viewerParticipants: [seat(0)],
    ...overrides,
  };
}

describe("my matches waiting on the viewer", () => {
  it("waits on the seat that is on the move", () => {
    expect(needsViewerAction(summary({ activeSlotIndex: 0 }))).toBe(true);
    expect(needsViewerAction(summary({ activeSlotIndex: 1 }))).toBe(false);
  });

  it("reads every seat a hotseat viewer holds", () => {
    const hotseat = summary({ activeSlotIndex: 1, viewerParticipants: [seat(0), seat(1)] });
    expect(needsViewerAction(hotseat)).toBe(true);
  });

  it("waits on a ranked pairing the viewer has not confirmed", () => {
    expect(
      needsViewerAction(
        summary({ phase: "pending", viewerParticipants: [seat(0, { ready: false })] }),
      ),
    ).toBe(true);
    expect(needsViewerAction(summary({ phase: "pending" }))).toBe(false);
  });

  it("waits on a lobby seat that is unready or has no CO", () => {
    expect(
      needsViewerAction(
        summary({ phase: "lobby", viewerParticipants: [seat(0, { ready: false })] }),
      ),
    ).toBe(true);
    expect(
      needsViewerAction(summary({ phase: "lobby", viewerParticipants: [seat(0, { coId: null })] })),
    ).toBe(true);
    expect(needsViewerAction(summary({ phase: "lobby" }))).toBe(false);
  });

  it("waits on nothing once the match is over", () => {
    expect(needsViewerAction(summary({ phase: "completed", activeSlotIndex: 0 }))).toBe(false);
    expect(needsViewerAction(summary({ phase: "starting", activeSlotIndex: 0 }))).toBe(false);
    expect(needsViewerAction(summary({ phase: "active", activeSlotIndex: null }))).toBe(false);
  });
});

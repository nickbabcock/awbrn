import { describe, expect, it } from "vitest";
import {
  formatMyMatchPhaseLabel,
  groupMyMatchRows,
  myMatchActionLabel,
  myMatchPhaseRank,
  ONGOING_MATCH_PHASES,
} from "./my_matches.ts";
import type { MatchPhase } from "./schemas.ts";

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

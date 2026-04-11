import { describe, expect, it } from "vitest";
import {
  formatMyMatchPhaseLabel,
  myMatchActionLabel,
  myMatchPhaseRank,
  ONGOING_MATCH_PHASES,
} from "./my_matches.ts";
import type { MatchPhase } from "./schemas.ts";

describe("my matches phases", () => {
  it("defines the ongoing match phases", () => {
    expect(ONGOING_MATCH_PHASES).toEqual(["draft", "lobby", "starting", "active"]);
  });

  it("orders active work before setup phases", () => {
    const phases: MatchPhase[] = ["draft", "lobby", "starting", "active"];
    expect(phases.sort((a, b) => myMatchPhaseRank(a) - myMatchPhaseRank(b))).toEqual([
      "active",
      "starting",
      "lobby",
      "draft",
    ]);
  });

  it("uses stable player-facing phase and action labels", () => {
    expect(formatMyMatchPhaseLabel("active")).toBe("Active");
    expect(myMatchActionLabel("active")).toBe("Open Match");
    expect(myMatchActionLabel("starting")).toBe("View Starting Match");
    expect(myMatchActionLabel("lobby")).toBe("Open Lobby");
  });
});

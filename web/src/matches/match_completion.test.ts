import { describe, expect, it } from "vitest";
import type { MatchResults } from "#/wasm/awbrn_server.js";
import { matchResultRows } from "./match_completion.ts";
import { placementMatchesOutcome, reasonMatchesOutcome } from "./match_results.ts";
import { defaultMatchClock, type MatchSetup } from "./schemas.ts";

function setup(userIds: string[]): MatchSetup {
  return {
    matchId: "match-1",
    mapId: "000000000001",
    revision: 1,
    map: {} as MatchSetup["map"],
    fogEnabled: false,
    startingFunds: 0,
    clock: defaultMatchClock,
    creatorUserId: userIds[0]!,
    players: userIds.map((userId, index) => ({
      userId,
      factionId: index,
      team: null,
      startingFunds: 0,
      coId: 1,
    })),
  };
}

const rout: MatchResults = {
  seats: [
    { slotIndex: 0, teamId: "player-0", outcome: "win", placement: 1, status: "active" },
    {
      slotIndex: 1,
      teamId: "player-1",
      outcome: "loss",
      placement: 2,
      reason: "rout",
      status: "eliminated",
    },
  ],
};

/** One person against one opponent the server plays. */
function aiSetup(): MatchSetup {
  const base = setup(["alice", "unused"]);
  return {
    ...base,
    players: [base.players[0]!, { ...base.players[1]!, userId: null, aiProfileId: "ai-hard-v1" }],
  };
}

describe("match completion", () => {
  it("writes one row for each seat, named by the user in that slot", () => {
    expect(matchResultRows(setup(["alice", "bob"]), rout)).toEqual([
      {
        matchId: "match-1",
        slotIndex: 0,
        userId: "alice",
        aiProfileId: null,
        teamId: "player-0",
        outcome: "win",
        placement: 1,
        reason: null,
        pool: null,
      },
      {
        matchId: "match-1",
        slotIndex: 1,
        userId: "bob",
        aiProfileId: null,
        teamId: "player-1",
        outcome: "loss",
        placement: 2,
        reason: "rout",
        pool: null,
      },
    ]);
  });

  it("gives a hotseat player a row for each slot they hold", () => {
    const rows = matchResultRows(setup(["alice", "alice"]), rout);
    expect(rows.map((row) => row.userId)).toEqual(["alice", "alice"]);
    expect(rows.map((row) => row.slotIndex)).toEqual([0, 1]);
  });

  it("copies a ranked pool into every result row", () => {
    const rankedSetup = { ...setup(["alice", "bob"]), pool: "async" as const, season: 1 };
    expect(matchResultRows(rankedSetup, rout).map((row) => row.pool)).toEqual(["async", "async"]);
  });

  it("drops a seat the setup does not hold, because a row needs a user", () => {
    expect(matchResultRows(setup(["alice"]), rout)).toHaveLength(1);
  });

  it("records a seat the server played, holding a profile where a user would be", () => {
    const rows = matchResultRows(aiSetup(), rout);
    expect(rows.map((row) => row.userId)).toEqual(["alice", null]);
    expect(rows.map((row) => row.aiProfileId)).toEqual([null, "ai-hard-v1"]);
  });

  /**
   * A rating is between people. A match the server took a seat in happened and
   * is recorded, but it is not one anybody's rating moves on, whatever the
   * match was opened as.
   */
  it("takes the pool off a match the server played a seat in", () => {
    const ranked = { ...aiSetup(), pool: "async" as const, season: 1 };
    expect(matchResultRows(ranked, rout).map((row) => row.pool)).toEqual([null, null]);
  });

  it("writes rows the result table accepts", () => {
    const draw: MatchResults = {
      seats: [
        {
          slotIndex: 0,
          teamId: "player-0",
          outcome: "draw",
          placement: 1,
          reason: "day-limit",
          status: "active",
        },
        {
          slotIndex: 1,
          teamId: "player-1",
          outcome: "draw",
          placement: 1,
          reason: "day-limit",
          status: "active",
        },
      ],
    };
    for (const results of [rout, draw]) {
      for (const row of matchResultRows(setup(["alice", "bob"]), results)) {
        expect(placementMatchesOutcome(row.outcome, row.placement)).toBe(true);
        expect(reasonMatchesOutcome(row.outcome, row.reason)).toBe(true);
      }
    }
  });
});

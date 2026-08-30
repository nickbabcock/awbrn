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

describe("match completion", () => {
  it("writes one row for each seat, named by the user in that slot", () => {
    expect(matchResultRows(setup(["alice", "bob"]), rout)).toEqual([
      {
        matchId: "match-1",
        slotIndex: 0,
        userId: "alice",
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

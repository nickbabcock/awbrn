import { describe, expect, it } from "vitest";
import type { MatchSetup } from "./schemas.ts";
import { initialMatchConnectionMessages } from "./match_protocol.ts";

const setup: MatchSetup = {
  matchId: "match_123",
  mapId: 162795,
  map: {
    Name: "Test Map",
    Author: "Andy",
    "Player Count": 2,
    "Published Date": "2026-04-12",
    "Size X": 2,
    "Size Y": 2,
    "Terrain Map": [
      [1, 2],
      [3, 4],
    ],
    "Predeployed Units": [],
  },
  players: [
    {
      userId: "user_1",
      factionId: 1,
      team: null,
      startingFunds: 1000,
      coId: 1,
    },
  ],
  fogEnabled: false,
  startingFunds: 1000,
  creatorUserId: "user_1",
};

describe("initial match connection messages", () => {
  it("sends the AWBW board before the connection acknowledgement", () => {
    expect(initialMatchConnectionMessages(setup, 0)).toEqual([
      {
        type: "initialBoard",
        mapId: 162795,
        map: setup.map,
      },
      {
        type: "connected",
        slotIndex: 0,
      },
    ]);
  });

  it("keeps spectator connections identified without changing the initial board", () => {
    expect(initialMatchConnectionMessages(setup, null)).toEqual([
      {
        type: "initialBoard",
        mapId: 162795,
        map: setup.map,
      },
      {
        type: "connected",
        slotIndex: null,
      },
    ]);
  });
});

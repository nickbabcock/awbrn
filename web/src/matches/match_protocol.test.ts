import { describe, expect, it } from "vitest";
import type { MatchSetup } from "./schemas.ts";
import {
  initialMatchConnectionMessages,
  type MatchGameState,
  type WasmActionResponse,
} from "./match_protocol.ts";

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

const gameState: MatchGameState = {
  viewerSlotIndex: 0,
  day: 1,
  activePlayerSlot: 0,
  phase: "PlayerTurn",
  myFunds: 1000,
  players: [{ slotIndex: 0, funds: 1000 }],
  units: [],
  terrain: [],
};

describe("initial match connection messages", () => {
  it("sends the AWBW board before the connection acknowledgement", () => {
    expect(initialMatchConnectionMessages(setup, 0, gameState)).toEqual([
      {
        type: "initialBoard",
        mapId: 162795,
        map: setup.map,
        gameState,
      },
      {
        type: "connected",
        slotIndex: 0,
      },
    ]);
  });

  it("keeps spectator connections identified without changing the initial board", () => {
    expect(initialMatchConnectionMessages(setup, null, null)).toEqual([
      {
        type: "initialBoard",
        mapId: 162795,
        map: setup.map,
        gameState: null,
      },
      {
        type: "connected",
        slotIndex: null,
      },
    ]);
  });

  it("sends a fog spectator notice before the connection acknowledgement", () => {
    expect(
      initialMatchConnectionMessages(setup, null, null, {
        type: "spectatorNotice",
        fogActive: true,
      }),
    ).toEqual([
      {
        type: "initialBoard",
        mapId: 162795,
        map: setup.map,
        gameState: null,
      },
      {
        type: "spectatorNotice",
        fogActive: true,
      },
      {
        type: "connected",
        slotIndex: null,
      },
    ]);
  });
});

describe("wasm action responses", () => {
  it("keeps route-ready websocket messages typed", () => {
    const response: WasmActionResponse = {
      storedActionEvent: {
        command: { type: "endTurn" },
        combatOutcome: null,
      },
      playerMessagesBySlot: new Map([
        [
          "0",
          {
            type: "playerUpdate",
            day: 2,
            activePlayerSlot: 1,
            phase: "PlayerTurn",
            players: [{ slotIndex: 0, funds: 900 }],
            unitsRevealed: [],
            unitsMoved: [
              {
                id: 7,
                path: [
                  { x: 0, y: 0 },
                  { x: 1, y: 0 },
                ],
                from: { x: 0, y: 0 },
                to: { x: 1, y: 0 },
              },
            ],
            unitsRemoved: [8],
            terrainRevealed: [],
            terrainChanged: [],
            combatEvents: [],
            captureEvents: [],
            turnChange: { newActivePlayerSlot: 1, newDay: null },
            fundsChanged: 900,
          },
        ],
      ]),
      spectatorMessage: {
        type: "spectatorNotice",
        fogActive: true,
      },
    };

    expect(response.playerMessagesBySlot.get("0")).toMatchObject({
      type: "playerUpdate",
      day: 2,
      activePlayerSlot: 1,
      players: [{ slotIndex: 0, funds: 900 }],
      unitsMoved: [{ id: 7, from: { x: 0, y: 0 }, to: { x: 1, y: 0 } }],
      unitsRemoved: [8],
      turnChange: { newActivePlayerSlot: 1, newDay: null },
      fundsChanged: 900,
    });
    expect(response.spectatorMessage).toEqual({
      type: "spectatorNotice",
      fogActive: true,
    });
  });
});

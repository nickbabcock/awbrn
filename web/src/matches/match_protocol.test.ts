import { describe, expect, expectTypeOf, it } from "vitest";
import type { MatchSetup } from "./schemas.ts";
import {
  initialMatchConnectionMessages,
  type ActivatePowerCommand,
  type EndTurnCommand,
  type LiveTransition,
  type MatchGameState,
  type WasmActionResponse,
} from "./match_protocol.ts";

describe("live match commands", () => {
  it("uses the server's tagged power command", () => {
    expectTypeOf<ActivatePowerCommand>().toEqualTypeOf<{
      type: "activatePower";
      level: "cop" | "scop";
    }>();
  });

  it("uses the server's tagged end-turn command", () => {
    expectTypeOf<EndTurnCommand>().toEqualTypeOf<{ type: "endTurn" }>();
  });
});

const setup: MatchSetup = {
  matchId: "match_123",
  mapId: "000000162795",
  revision: 1,
  map: {
    map_format: 1,
    width: 2,
    height: 2,
    terrain: [1, 3, 2, 4],
    units: [],
    metadata: { name: "Test Map", author: "Andy", player_count: 2 },
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

const observation: LiveTransition["post"] = {
  ruleset: { id: "awbw", revision: "2026-07-10" },
  recipient: "p1",
  settings: {
    fog: false,
    income_per_property: 1000,
    starting_funds: 1000,
    powers: "enabled",
    tags: false,
    weather: "clear",
    lab_units: [],
    unit_bans: [],
    commander_bans: { lead: [], backup: [] },
    capture_limit: undefined,
    day_limit: undefined,
    unit_limit: undefined,
  },
  board: {
    width: 1,
    height: 1,
    tiles: [[{ terrain: "plain", visibility: "visible" }]],
  },
  teams: [{ id: "t1", status: "active" }],
  players: [
    {
      id: "p1",
      team: "t1",
      relation: "self",
      funds: 1000,
      status: "active",
      commanders: [{ id: "andy", active: true, power_charge: 0, power_uses: 0 }],
      power_state: { type: "none" },
    },
  ],
  turn: {
    day: 1,
    active_player: "p1",
    phase: "unit-action",
    order: ["p1"],
    position: 0,
  },
  weather: { kind: "clear", remaining_turns: 0 },
  units: [],
  match: { status: "active", own_team_offers: [] },
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
  observation,
};

describe("initial match connection messages", () => {
  it("sends the AWBW board before the connection acknowledgement", () => {
    expect(initialMatchConnectionMessages(setup, 0, gameState)).toEqual([
      {
        type: "initialBoard",
        mapId: "000000162795",
        revision: 1,
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
        mapId: "000000162795",
        revision: 1,
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
        mapId: "000000162795",
        revision: 1,
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
      },
      playerMessagesBySlot: {
        "0": {
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
          transition: { post: observation, events: [] },
        },
      },
      spectatorMessage: {
        type: "spectatorNotice",
        fogActive: true,
      },
    };

    expect(response.playerMessagesBySlot["0"]).toMatchObject({
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

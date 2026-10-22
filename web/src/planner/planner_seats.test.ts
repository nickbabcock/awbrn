import { describe, expect, it } from "vitest";
import type { AwbwGameArmy, AwbwGameSnapshot } from "#/awbw/schemas.ts";
import { DEFAULT_SEAT, seatSetup, setupFromAwbwGame } from "./planner_seats.ts";

/** The charge of a star after 0, 1 and 2 power uses. */
const STAR_CHARGES = [9000, 10800, 12600];
const RACHEL = 28;
const JESS = 14;
const GRIT = 2;

function army(overrides: Partial<AwbwGameArmy>): AwbwGameArmy {
  return {
    playerId: 1,
    username: "Player",
    factionCode: "os",
    team: "A",
    order: 1,
    eliminated: false,
    coId: JESS,
    coPower: 0,
    powerOn: "none",
    copBaseCost: 270000,
    copCost: 270000,
    scopBaseCost: 540000,
    scopCost: 540000,
    funds: null,
    ...overrides,
  };
}

function game(armies: AwbwGameArmy[]): AwbwGameSnapshot {
  return { gameId: 9, mapId: 186800, activePlayerId: 1, armies };
}

describe("planner setup from an AWBW game", () => {
  const armies = [
    army({ playerId: 1, coId: JESS, coPower: 470500, scopCost: 648000 }),
    army({ playerId: 2, team: "B", coId: RACHEL, coPower: 477000, scopCost: 648000 }),
    army({
      playerId: 3,
      team: "B",
      coId: GRIT,
      coPower: 131500,
      powerOn: "cop",
      scopCost: 756000,
      funds: 8000,
    }),
  ];

  it("takes each army's CO, power, team, the map and the army to move", () => {
    const setup = setupFromAwbwGame(game(armies), [1, 2, 3], {}, STAR_CHARGES, [RACHEL]);

    expect(setup?.mapId).toBe("186800");
    expect(setup?.firstPlayerId).toBe("1");
    expect(setup?.seats[2]).toEqual({
      coId: RACHEL,
      // Fog hides Rachel's funds, so they stay as they were.
      funds: "0",
      // 47,700 charge at 10,800 a star after one use.
      powerStars: "4.42",
      powerUses: 1,
      powerOn: "none",
      team: "B",
      importedCharge: 47700,
    });
    expect(setup?.seats[3]).toMatchObject({ powerUses: 2, powerOn: "cop", funds: "8000" });
  });

  it("previews the missiles of the enemy of the army to move", () => {
    const setup = setupFromAwbwGame(game(armies), [1, 2, 3], {}, STAR_CHARGES, [RACHEL]);

    expect(setup?.previewedPlayerId).toBe("2");
  });

  it("keeps the exact charge until the stars change", () => {
    const setup = setupFromAwbwGame(game(armies), [1, 2, 3], {}, STAR_CHARGES, [RACHEL]);
    const rachel = setup!.seats[2];

    expect(seatSetup(2, rachel, STAR_CHARGES).powerCharge).toBe(47700);
    const typed = { ...rachel, powerStars: "5", importedCharge: undefined };
    expect(seatSetup(2, typed, STAR_CHARGES).powerCharge).toBe(54000);
    expect(seatSetup(4, { ...DEFAULT_SEAT, powerStars: "2.5" }, STAR_CHARGES).powerCharge).toBe(
      22500,
    );
  });

  it("refuses a game that holds none of the planner's armies", () => {
    expect(setupFromAwbwGame(game(armies), [7, 8], {}, STAR_CHARGES, [RACHEL])).toBeNull();
  });
});

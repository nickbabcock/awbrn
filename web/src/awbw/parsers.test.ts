import { describe, expect, it } from "vitest";
import { parseAwbwGame, parseAwbwGameId } from "./parsers.ts";

/** One army of `playersInfo`, in the shape that `game.php` embeds. */
function pagePlayer(overrides: Record<string, unknown>) {
  return {
    users_username: "Player",
    users_id: 1,
    players_id: 1,
    players_team: "A",
    players_countries_id: 1,
    players_eliminated: "N",
    players_co_id: 14,
    co_name: "Jess",
    co_max_power: 270000,
    co_max_spower: 540000,
    players_co_power: 0,
    players_co_power_on: "N",
    players_co_max_power: 270000,
    players_co_max_spower: 540000,
    players_funds: "?",
    countries_code: "os",
    countries_name: "Orange Star",
    players_order: 1,
    players_income: 1500,
    ...overrides,
  };
}

function gamePage(players: Record<string, unknown>[], extra = "") {
  const playersInfo = Object.fromEntries(players.map((player) => [player.players_id, player]));
  return `<html><script>
let gameDay = 13;
let playersInfo = ${JSON.stringify(playersInfo)};
let currentTurn = 4110262;
${extra}
</script>
<a href="prevmaps.php?maps_id=186800">3v3 map</a></html>`;
}

describe("AWBW game page", () => {
  it("reads each army's CO, power, team and funds in turn order", () => {
    const html = gamePage([
      pagePlayer({
        players_id: 4110379,
        // A name can hold the characters that end a script assignment.
        users_username: 'Rock}; let "x',
        players_team: "B",
        players_order: 2,
        players_co_id: 28,
        players_co_power: 477000,
        players_co_max_power: 324000,
        players_co_max_spower: 648000,
        players_funds: "12000",
        countries_code: "bm",
      }),
      pagePlayer({
        players_id: 4110262,
        players_co_power: 470500,
        players_co_power_on: "Y",
      }),
    ]);

    expect(parseAwbwGame(html, 1710706)).toEqual({
      gameId: 1710706,
      mapId: 186800,
      activePlayerId: 4110262,
      armies: [
        {
          playerId: 4110262,
          username: "Player",
          factionCode: "os",
          team: "A",
          order: 1,
          eliminated: false,
          coId: 14,
          coPower: 470500,
          powerOn: "cop",
          copBaseCost: 270000,
          copCost: 270000,
          scopBaseCost: 540000,
          scopCost: 540000,
          funds: null,
        },
        {
          playerId: 4110379,
          username: 'Rock}; let "x',
          factionCode: "bm",
          team: "B",
          order: 2,
          eliminated: false,
          coId: 28,
          coPower: 477000,
          powerOn: "none",
          copBaseCost: 270000,
          copCost: 324000,
          scopBaseCost: 540000,
          scopCost: 648000,
          funds: 12000,
        },
      ],
    });
  });

  it("finds no game on a page without players", () => {
    expect(parseAwbwGame("<html>Game not found</html>", 5)).toBeNull();
    expect(parseAwbwGame(gamePage([pagePlayer({ players_co_power_on: "?" })]), 5)).toBeNull();
  });
});

describe("AWBW game ID", () => {
  it("reads a game link or a bare number", () => {
    expect(parseAwbwGameId("https://awbw.amarriner.com/game.php?games_id=1710706")).toBe(1710706);
    expect(parseAwbwGameId(" awbw.amarriner.com/game.php?ndx=0&games_id=42 ")).toBe(42);
    expect(parseAwbwGameId("1710706")).toBe(1710706);
  });

  it("refuses other text", () => {
    expect(parseAwbwGameId("https://example.com/game.php?games_id=1")).toBeNull();
    expect(parseAwbwGameId("https://awbw.amarriner.com/prevmaps.php?maps_id=1")).toBeNull();
    expect(parseAwbwGameId("game 12")).toBeNull();
    expect(parseAwbwGameId("0")).toBeNull();
  });
});

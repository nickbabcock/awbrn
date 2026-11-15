import { describe, expect, it } from "vitest";
import { seasonPlaces, thinPoints, topCos } from "./profile_read.ts";

describe("topCos", () => {
  it("ranks COs by games and stands each on the army fielded with it most", () => {
    const cos = topCos([
      { coId: 1, factionId: 1, outcome: "win", seats: 3 },
      { coId: 1, factionId: 2, outcome: "loss", seats: 4 },
      { coId: 1, factionId: 2, outcome: "draw", seats: 1 },
      { coId: 2, factionId: 3, outcome: "win", seats: 9 },
      { coId: 3, factionId: 1, outcome: "loss", seats: 2 },
      { coId: 4, factionId: 1, outcome: "win", seats: 1 },
    ]);
    expect(cos).toEqual([
      { coId: 2, factionId: 3, wins: 9, losses: 0, draws: 0 },
      { coId: 1, factionId: 2, wins: 3, losses: 4, draws: 1 },
      { coId: 3, factionId: 1, wins: 0, losses: 2, draws: 0 },
    ]);
  });
});

describe("seasonPlaces", () => {
  const standings = new Map([
    [1, { rank: 4, rating: 1580, deviation: 90, seasonMatches: 14 }],
    [3, { rank: 12, rating: 1540, deviation: 180, seasonMatches: 3 }],
  ]);

  it("reads closed seasons from their standings and the open one from the live ladder", () => {
    const places = seasonPlaces({
      currentSeason: 4,
      firstSeason: 1,
      games: new Map([
        [1, 14],
        [2, 5],
        [3, 3],
        [4, 2],
      ]),
      lastSeason: 4,
      live: { place: 7, rating: 1610, deviation: 120 },
      standings,
    });
    expect(places).toEqual([
      { season: 1, games: 14, place: 4, rating: 1580, isProvisional: false },
      // Played, but no standings row: no place.
      { season: 2, games: 5, place: null, rating: null, isProvisional: false },
      // Provisional as the season froze it.
      { season: 3, games: 3, place: 12, rating: 1540, isProvisional: true },
      { season: 4, games: 2, place: 7, rating: 1610, isProvisional: false },
    ]);
  });

  it("starts at the pool's first season and leaves the open season unplaced off the ladder", () => {
    const places = seasonPlaces({
      currentSeason: 4,
      firstSeason: 3,
      games: new Map([[3, 3]]),
      lastSeason: 4,
      live: { place: null, rating: 1540, deviation: 280 },
      standings,
    });
    expect(places.map((entry) => [entry.season, entry.place, entry.games])).toEqual([
      [3, 12, 3],
      [4, null, 0],
    ]);
    expect(places[1]!.isProvisional).toBe(true);
  });
});

describe("thinPoints", () => {
  const history = (ratings: number[]) =>
    ratings.map((rating, index): [number, number] => [index, rating]);

  it("keeps a short history whole", () => {
    expect(thinPoints(history([1, 2, 3]), 5)).toEqual(history([1, 2, 3]));
  });

  it("thins evenly and always keeps the first and last", () => {
    const thinned = thinPoints(history(Array.from({ length: 1001 }, () => 1500)), 11);
    expect(thinned.map(([at]) => at)).toEqual([
      0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000,
    ]);
  });

  it("keeps the peak and the low point that the stride would skip", () => {
    const ratings = Array.from({ length: 1001 }, () => 1500);
    ratings[555] = 2200;
    ratings[777] = 1300;
    const thinned = thinPoints(history(ratings), 11);
    expect(thinned.map(([at]) => at)).toEqual([
      0, 100, 200, 300, 400, 500, 555, 600, 700, 777, 800, 900, 1000,
    ]);
  });
});

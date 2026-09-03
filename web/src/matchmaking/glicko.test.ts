import { describe, expect, it } from "vitest";
import {
  MAX_GLICKO_DEVIATION,
  MIN_GLICKO_DEVIATION,
  updateRating,
  type GlickoState,
} from "./glicko.ts";

const UNRATED: GlickoState = { rating: 1500, deviation: 350, volatility: 0.06 };

describe("updateRating", () => {
  /*
   * The worked example from Glickman's paper, section 3. A player at 1500 with
   * a deviation of 200 beats 1400, then loses to 1550 and to 1700. The paper
   * gives mu' = -0.2069 and phi' = 0.8722 on the Glicko-2 scale, which is
   * 1464.06 and 151.52 on the scale a player reads.
   */
  it("agrees with the worked example in the paper", () => {
    const after = updateRating({ rating: 1500, deviation: 200, volatility: 0.06 }, [
      { rating: 1400, deviation: 30, score: 1 },
      { rating: 1550, deviation: 100, score: 0 },
      { rating: 1700, deviation: 300, score: 0 },
    ]);

    expect(after.rating).toBeCloseTo(1464.06, 1);
    expect(after.deviation).toBeCloseTo(151.52, 1);
    // The paper prints the volatility to four figures.
    expect(after.volatility).toBeCloseTo(0.05999, 4);
  });

  it("raises the winner and lowers the loser by the same amount", () => {
    const state: GlickoState = { rating: 1500, deviation: 80, volatility: 0.06 };
    const winner = updateRating(state, [{ rating: 1500, deviation: 80, score: 1 }]);
    const loser = updateRating(state, [{ rating: 1500, deviation: 80, score: 0 }]);

    expect(winner.rating).toBeGreaterThan(1500);
    expect(loser.rating).toBeLessThan(1500);
    expect(winner.rating - 1500).toBeCloseTo(1500 - loser.rating, 6);
  });

  it("leaves an even draw where it found it", () => {
    const after = updateRating({ rating: 1500, deviation: 80, volatility: 0.06 }, [
      { rating: 1500, deviation: 80, score: 0.5 },
    ]);

    expect(after.rating).toBeCloseTo(1500, 6);
  });

  it("moves a draw against a stronger player upwards", () => {
    const after = updateRating({ rating: 1400, deviation: 80, volatility: 0.06 }, [
      { rating: 1700, deviation: 80, score: 0.5 },
    ]);

    expect(after.rating).toBeGreaterThan(1400);
  });

  it("moves a rating less when the opponent is unsure than when they are known", () => {
    const state: GlickoState = { rating: 1500, deviation: 80, volatility: 0.06 };
    const againstKnown = updateRating(state, [{ rating: 1500, deviation: 30, score: 1 }]);
    const againstUnsure = updateRating(state, [{ rating: 1500, deviation: 300, score: 1 }]);

    expect(againstKnown.rating - 1500).toBeGreaterThan(againstUnsure.rating - 1500);
  });

  it("moves an unsure rating further than a settled one", () => {
    const unsure = updateRating({ rating: 1500, deviation: 300, volatility: 0.06 }, [
      { rating: 1500, deviation: 80, score: 1 },
    ]);
    const settled = updateRating({ rating: 1500, deviation: 40, volatility: 0.06 }, [
      { rating: 1500, deviation: 80, score: 1 },
    ]);

    expect(unsure.rating - 1500).toBeGreaterThan(settled.rating - 1500);
  });

  it("makes a rating more certain after a match", () => {
    const after = updateRating(UNRATED, [{ rating: 1500, deviation: 80, score: 1 }]);
    expect(after.deviation).toBeLessThan(UNRATED.deviation);
  });

  it("only loosens the deviation when nobody was played", () => {
    const after = updateRating({ rating: 1720, deviation: 90, volatility: 0.06 }, []);

    expect(after.rating).toBe(1720);
    expect(after.deviation).toBeGreaterThan(90);
    expect(after.volatility).toBe(0.06);
  });

  it("holds the deviation inside its bounds", () => {
    const floor = updateRating({ rating: 1500, deviation: 31, volatility: 0.06 }, [
      { rating: 1500, deviation: 30, score: 1 },
    ]);
    expect(floor.deviation).toBeGreaterThanOrEqual(MIN_GLICKO_DEVIATION);

    const ceiling = updateRating({ rating: 1500, deviation: 350, volatility: 0.06 }, []);
    expect(ceiling.deviation).toBeLessThanOrEqual(MAX_GLICKO_DEVIATION);
  });

  it("raises the volatility after a result the rating did not expect", () => {
    const after = updateRating({ rating: 1900, deviation: 60, volatility: 0.06 }, [
      { rating: 1300, deviation: 60, score: 0 },
    ]);

    expect(after.volatility).toBeGreaterThan(0.06);
  });
});

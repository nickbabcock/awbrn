import { describe, expect, it } from "vitest";
import { seatArmies } from "./seat_armies.ts";

const OS = 1;
const BM = 2;
const GE = 3;
const YC = 4;
const ALL = [OS, BM, GE, YC];

describe("seatArmies", () => {
  it("gives open seats the map's armies", () => {
    expect(seatArmies([OS, BM], new Map(), ALL)).toEqual([OS, BM]);
  });

  it("gives an open seat the side a holder left when its own army is taken", () => {
    expect(seatArmies([OS, BM], new Map([[0, BM]]), ALL)).toEqual([BM, OS]);
    expect(seatArmies([OS, BM, GE], new Map([[2, OS]]), ALL)).toEqual([GE, BM, OS]);
  });

  it("leaves the map's armies alone when a holder picks one off the map", () => {
    expect(seatArmies([OS, BM], new Map([[1, YC]]), ALL)).toEqual([OS, YC]);
  });

  it("falls back to the first army nobody holds", () => {
    expect(
      seatArmies(
        [OS, BM],
        new Map([
          [0, BM],
          [1, OS],
        ]),
        ALL,
      ),
    ).toEqual([BM, OS]);
    expect(
      seatArmies(
        [OS, BM, GE],
        new Map([
          [0, BM],
          [1, GE],
        ]),
        ALL,
      ),
    ).toEqual([BM, GE, OS]);
  });
});

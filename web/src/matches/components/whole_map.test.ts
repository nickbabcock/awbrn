import { describe, expect, it } from "vitest";
import { followBoardView, openWholeMap, WHOLE_MAP_CLOSED } from "./whole_map.ts";

const left = { x: 4, y: 6, width: 20, height: 12 };
const atTwo = { scale: 2, fitScale: 0.71 };
const atFit = { scale: 0.71, fitScale: 0.71 };
const wheeled = { scale: 0.89, fitScale: 0.71 };

describe("followBoardView", () => {
  it("keeps the saved view while the fit has not arrived", () => {
    // The commit after the open still holds the view the player left.
    const opened = openWholeMap(left);
    expect(followBoardView(opened, atTwo)).toBe(opened);
  });

  it("settles the open once the board reaches the fit scale", () => {
    const settled = followBoardView(openWholeMap(left), atFit);
    expect(settled).toEqual({ left, awaitingFit: false });
    expect(followBoardView(settled, atFit)).toBe(settled);
  });

  it("drops the way back when the board zooms in after the fit", () => {
    const opened = openWholeMap(left);
    const pending = followBoardView(opened, atTwo);
    const settled = followBoardView(pending, atFit);
    expect(settled.left).toEqual(left);
    expect(followBoardView(settled, wheeled)).toBe(WHOLE_MAP_CLOSED);
  });

  it("drops the way back when the board goes", () => {
    expect(followBoardView(openWholeMap(left), null)).toBe(WHOLE_MAP_CLOSED);
  });

  it("leaves a closed whole map alone", () => {
    expect(followBoardView(WHOLE_MAP_CLOSED, atTwo)).toBe(WHOLE_MAP_CLOSED);
    expect(followBoardView(WHOLE_MAP_CLOSED, null)).toBe(WHOLE_MAP_CLOSED);
  });
});

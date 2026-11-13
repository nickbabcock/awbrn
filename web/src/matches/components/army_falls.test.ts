import { describe, expect, it } from "vitest";
import { fallDayFromOutline, fallReason } from "./army_falls.ts";

/** One boundary per action: `[day after, acting seat]`, the opening first. */
function outline(actions: [number, number][]) {
  return [
    { day: actions[0]?.[0] ?? 1, actingSlot: null },
    ...actions.map(([day, actingSlot]) => ({ day, actingSlot })),
  ];
}

describe("fallDayFromOutline", () => {
  const order = [0, 1, 2];

  it("names the day when every action after the seat's last is on it", () => {
    // Seat 0 ends day 3; seats 1 and 2 play day 3, and seat 2's end turn
    // starts day 4, where seat 0 would have played next.
    const boundaries = outline([
      [3, 0],
      [3, 1],
      [3, 1],
      [3, 2],
      [4, 2],
      [4, 1],
    ]);
    expect(fallDayFromOutline(boundaries, 0, order)).toBe(3);
  });

  it("says nothing when the stretch crosses into the next day", () => {
    // Seat 1 ends day 3; seat 2 plays day 3, then seat 0 plays day 4 before
    // seat 1's turn would have come.
    const boundaries = outline([
      [3, 1],
      [3, 2],
      [4, 2],
      [4, 0],
      [4, 0],
      [4, 2],
    ]);
    expect(fallDayFromOutline(boundaries, 1, order)).toBeNull();
  });

  it("reads the day an action was taken on, not the day it lands on", () => {
    // The last seat in the order ends day 3, landing on day 4; the others
    // play day 4 and the seat is passed over.
    const boundaries = outline([
      [3, 2],
      [4, 2],
      [4, 0],
      [4, 1],
      [5, 1],
    ]);
    expect(fallDayFromOutline(boundaries, 2, order)).toBeNull();
    expect(
      fallDayFromOutline(
        outline([
          [3, 1],
          [3, 2],
        ]),
        2,
        order,
      ),
    ).toBe(3);
  });

  it("knows nothing of a seat that never acted", () => {
    expect(fallDayFromOutline(outline([[1, 0]]), 1, order)).toBeNull();
    expect(fallDayFromOutline(outline([[1, 0]]), 7, order)).toBeNull();
  });
});

describe("fallReason", () => {
  it("names each way out and nothing for a seat still in", () => {
    expect(fallReason("active")).toBeNull();
    expect(fallReason("resigned")).toBe("Resigned");
    expect(fallReason("timed-out")).toBe("Clock ran out");
    expect(fallReason("eliminated")).toBe("Defeated");
  });
});

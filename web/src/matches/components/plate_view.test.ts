import { describe, expect, it } from "vitest";
import type { ArmyView } from "./Armies.tsx";
import { plateView } from "./plate_view.ts";

function view(
  playerId: number,
  options: Partial<{ active: boolean; viewer: boolean; out: boolean }>,
) {
  return {
    army: {
      isActive: options.active ?? false,
      entry: { playerId, eliminated: options.out ?? false },
    },
    isViewer: options.viewer ?? false,
    clock: null,
  } as unknown as ArmyView;
}

describe("plateView", () => {
  const views = [view(0, {}), view(1, { viewer: true }), view(2, { active: true })];

  it("shows your army live", () => {
    expect(plateView(views, { isReviewing: false, moverSlot: 2 })?.army.entry.playerId).toBe(1);
  });

  it("shows the mover once you are out", () => {
    const out = [view(0, {}), view(1, { viewer: true, out: true }), view(2, { active: true })];
    expect(plateView(out, { isReviewing: false, moverSlot: 2 })?.army.entry.playerId).toBe(2);
  });

  it("shows the army that moved at the reviewed moment", () => {
    // The live mover is army 0; the roster read at the moment says army 2.
    expect(plateView(views, { isReviewing: true, moverSlot: 0 })?.army.entry.playerId).toBe(2);
  });
});

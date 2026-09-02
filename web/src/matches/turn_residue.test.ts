import { describe, expect, it } from "vitest";
import { describeTurnResidue } from "./turn_residue";

describe("describeTurnResidue", () => {
  it("keeps unknown readiness separate from an empty turn", () => {
    expect(describeTurnResidue({ idleUnits: 0, freeSites: 0 }, false)).toBeNull();
    expect(describeTurnResidue(undefined, false)).toBeUndefined();
  });

  it("counts one of a thing in the singular", () => {
    expect(describeTurnResidue({ idleUnits: 1, freeSites: 0 }, false)).toBe(
      "1 unit has not moved. Ending the turn cannot be undone.",
    );
    expect(describeTurnResidue({ idleUnits: 0, freeSites: 1 }, false)).toBe(
      "1 base can still build. Ending the turn cannot be undone.",
    );
  });

  it("names everything the turn would leave behind", () => {
    expect(describeTurnResidue({ idleUnits: 3, freeSites: 2 }, true)).toBe(
      "3 units have not moved, 2 bases can still build and your CO power is ready. " +
        "Ending the turn cannot be undone.",
    );
  });

  it("asks about a charged power on its own", () => {
    expect(describeTurnResidue({ idleUnits: 0, freeSites: 0 }, true)).toBe(
      "Your CO power is ready. Ending the turn cannot be undone.",
    );
  });
});

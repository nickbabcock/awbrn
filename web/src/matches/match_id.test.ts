import { describe, expect, it } from "vitest";

import { generateMatchId, MATCH_ID_LENGTH } from "./match_id";

describe("generateMatchId", () => {
  it("emits fixed-width lowercase base36 ids", () => {
    const matchId = generateMatchId();

    expect(matchId).toHaveLength(MATCH_ID_LENGTH);
    expect(matchId).toMatch(/^[0-9a-z]+$/);
  });

  it("produces diverse values", () => {
    const ids = new Set<string>();

    for (let index = 0; index < 1_000; index += 1) {
      ids.add(generateMatchId());
    }

    expect(ids.size).toBe(1_000);
  });
});

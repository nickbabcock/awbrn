import { describe, expect, it } from "vitest";
import { buildTurnDigest, parsePlayerClientMessage, PUSH_DIGEST_LIMIT } from "./player_protocol.ts";

function waiting(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    matchId: `match${index}`,
    matchName: `Match ${index}`,
  }));
}

describe("buildTurnDigest", () => {
  it("sends a player with one waiting match straight to it", () => {
    const digest = buildTurnDigest([{ matchId: "abc", matchName: "Sand Island" }]);

    expect(digest.title).toBe("Your turn");
    expect(digest.body).toBe("Sand Island");
    expect(digest.url).toBe("/matches/abc");
    expect(digest.total).toBe(1);
  });

  it("sends a player with several to the list, because it cannot open them all", () => {
    const digest = buildTurnDigest(waiting(2));

    expect(digest.title).toBe("Your turn in 2 matches");
    expect(digest.body).toBe("Match 0, Match 1");
    expect(digest.url).toBe("/my/matches");
  });

  it("names as many as it draws and counts the rest", () => {
    const digest = buildTurnDigest(waiting(PUSH_DIGEST_LIMIT + 2));

    expect(digest.total).toBe(PUSH_DIGEST_LIMIT + 2);
    expect(digest.body).toBe("Match 0, Match 1, Match 2 and 2 more");
    expect(digest.title).toBe(`Your turn in ${PUSH_DIGEST_LIMIT + 2} matches`);
  });
});

describe("parsePlayerClientMessage", () => {
  it("takes a visibility report", () => {
    expect(parsePlayerClientMessage({ type: "visibility", visible: false })).toEqual({
      type: "visibility",
      visible: false,
    });
  });

  it("refuses anything else a tab might send", () => {
    expect(parsePlayerClientMessage({ type: "visibility" })).toBeNull();
    expect(parsePlayerClientMessage({ type: "visibility", visible: "yes" })).toBeNull();
    expect(parsePlayerClientMessage({ type: "turnStarted" })).toBeNull();
    expect(parsePlayerClientMessage(null)).toBeNull();
    expect(parsePlayerClientMessage("visibility")).toBeNull();
  });
});

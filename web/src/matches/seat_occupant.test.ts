import { describe, expect, it } from "vitest";
import { isServerPlayed, occupantColumns, occupantUserId, seatOccupant } from "./seat_occupant.ts";

describe("who holds a seat", () => {
  it("reads a person off the column that names one", () => {
    expect(seatOccupant({ userId: "alice", aiProfileId: null })).toEqual({
      kind: "human",
      userId: "alice",
    });
  });

  it("reads an opponent off the column that names one", () => {
    expect(seatOccupant({ userId: null, aiProfileId: "ai-hard-v1" })).toEqual({
      kind: "ai",
      profileId: "ai-hard-v1",
    });
  });

  it("writes back the columns it read", () => {
    for (const columns of [
      { userId: "alice", aiProfileId: null },
      { userId: null, aiProfileId: "ai-easy-v1" },
    ]) {
      expect(occupantColumns(seatOccupant(columns)!)).toEqual(columns);
    }
  });

  /**
   * The check constraint makes this row impossible, so what matters is that a
   * page which somehow reads one draws a seat rather than throwing in the
   * middle of a lobby.
   */
  it("holds nobody when neither column names anyone", () => {
    expect(seatOccupant({ userId: null, aiProfileId: null })).toBeNull();
    expect(occupantUserId(null)).toBeNull();
  });

  it("does not take an opponent this build has no profile for", () => {
    expect(seatOccupant({ userId: null, aiProfileId: "ai-retired-v0" })).toBeNull();
    expect(isServerPlayed({ userId: null, aiProfileId: "ai-retired-v0" })).toBe(false);
  });

  it("says which seats the server owes a turn", () => {
    expect(isServerPlayed({ userId: null, aiProfileId: "ai-standard-v1" })).toBe(true);
    expect(isServerPlayed({ userId: "alice", aiProfileId: null })).toBe(false);
  });

  it("finds the person in a seat, and none in the server's", () => {
    expect(occupantUserId({ kind: "human", userId: "alice" })).toBe("alice");
    expect(occupantUserId({ kind: "ai", profileId: "ai-easy-v1" })).toBeNull();
  });
});

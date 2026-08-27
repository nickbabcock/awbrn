import { describe, expect, it } from "vitest";
import { actorFromRole } from "#/auth/actor.ts";
import { matchViewAnyGrant, matchVoidGrant } from "./match_authz.ts";

const player = actorFromRole("u-player", "user");
const moderator = actorFromRole("u-mod", "moderator");

describe("matchVoidGrant", () => {
  it("is held by a moderator alone", () => {
    expect(matchVoidGrant(moderator)).toBe("moderator");
    expect(matchVoidGrant(player)).toBe(null);
    expect(matchVoidGrant(null)).toBe(null);
  });

  it("has no ownership branch, so a loser cannot void their own loss", () => {
    // The creator of a match is still only a player here.
    expect(matchVoidGrant(actorFromRole("u-creator", "user"))).toBe(null);
  });
});

describe("matchViewAnyGrant", () => {
  it("reaches past a private match for a moderator alone", () => {
    expect(matchViewAnyGrant(moderator)).toBe("moderator");
    expect(matchViewAnyGrant(player)).toBe(null);
    expect(matchViewAnyGrant(null)).toBe(null);
  });
});

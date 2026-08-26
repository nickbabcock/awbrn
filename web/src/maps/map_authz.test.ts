import { describe, expect, it } from "vitest";
import { actorFromRole } from "#/auth/actor.ts";
import { mapRankGrant, mapTagGrant } from "./map_authz.ts";

const author = actorFromRole("u-author", "user");
const stranger = actorFromRole("u-stranger", "user");
const moderator = actorFromRole("u-mod", "moderator");
const authorMod = actorFromRole("u-author", "moderator");

const ownedMap = { authorUserId: "u-author" };
const importedMap = { authorUserId: null };

describe("mapTagGrant", () => {
  it("lets the author tag the map they wrote", () => {
    expect(mapTagGrant(ownedMap, author)).toBe("owner");
  });

  it("keeps a stranger off a map they did not write", () => {
    expect(mapTagGrant(ownedMap, stranger)).toBe(null);
    expect(mapTagGrant(importedMap, stranger)).toBe(null);
  });

  it("lets a moderator past ownership, and says that is why", () => {
    expect(mapTagGrant(ownedMap, moderator)).toBe("moderator");
    expect(mapTagGrant(importedMap, moderator)).toBe("moderator");
  });

  it("reads ownership before the role", () => {
    // A moderator tagging their own map is not moderating, so the screen does
    // not ask them for a reason and the log does not gain a row.
    expect(mapTagGrant(ownedMap, authorMod)).toBe("owner");
  });

  it("refuses a visitor who is not signed in", () => {
    expect(mapTagGrant(ownedMap, null)).toBe(null);
  });
});

describe("mapRankGrant", () => {
  it("is a moderator act with no owner who could do it instead", () => {
    expect(mapRankGrant(ownedMap, moderator)).toBe("moderator");
    expect(mapRankGrant(importedMap, moderator)).toBe("moderator");
    expect(mapRankGrant(ownedMap, author)).toBe(null);
    expect(mapRankGrant(ownedMap, null)).toBe(null);
  });

  it("refuses a moderator the map they wrote", () => {
    // A rank is this site's judgement of a map, so the one thing the rule
    // stops is an author sitting in judgement of their own work.
    expect(mapRankGrant(ownedMap, authorMod)).toBe(null);
  });

  it("still lets that moderator rank a map somebody else wrote", () => {
    expect(mapRankGrant({ authorUserId: "u-stranger" }, authorMod)).toBe("moderator");
    expect(mapRankGrant(importedMap, authorMod)).toBe("moderator");
  });
});

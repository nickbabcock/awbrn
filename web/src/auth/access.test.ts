import { describe, expect, it } from "vitest";
import { DEFAULT_ROLE, isRoleName, ROLE_NAMES, roleAllows } from "./access.ts";

describe("roleAllows", () => {
  it("gives a player what every signed-in player holds", () => {
    expect(roleAllows("user", { map: ["import"] })).toBe(true);
    expect(roleAllows("user", { map: ["tag"] })).toBe(true);
  });

  it("keeps curation and abuse tools away from a player", () => {
    expect(roleAllows("user", { map: ["rank"] })).toBe(false);
    expect(roleAllows("user", { map: ["edit-any"] })).toBe(false);
    expect(roleAllows("user", { match: ["void"] })).toBe(false);
    expect(roleAllows("user", { user: ["ban"] })).toBe(false);
  });

  it("reads an empty column as the default role", () => {
    expect(roleAllows(null, { map: ["import"] })).toBe(
      roleAllows(DEFAULT_ROLE, { map: ["import"] }),
    );
    expect(roleAllows(undefined, { map: ["rank"] })).toBe(false);
  });

  it("gives a moderator curation and the abuse tools", () => {
    expect(roleAllows("moderator", { map: ["rank"] })).toBe(true);
    expect(roleAllows("moderator", { map: ["edit-any"] })).toBe(true);
    expect(roleAllows("moderator", { match: ["void", "view-any"] })).toBe(true);
    expect(roleAllows("moderator", { user: ["ban", "list"] })).toBe(true);
  });

  it("stops a moderator short of the rest of the admin plugin", () => {
    expect(roleAllows("moderator", { user: ["set-role"] })).toBe(false);
    expect(roleAllows("moderator", { user: ["impersonate"] })).toBe(false);
    expect(roleAllows("moderator", { user: ["delete"] })).toBe(false);
  });

  it("gives an admin everything a moderator holds and more", () => {
    expect(roleAllows("admin", { map: ["rank"] })).toBe(true);
    expect(roleAllows("admin", { match: ["void"] })).toBe(true);
    expect(roleAllows("admin", { user: ["set-role"] })).toBe(true);
  });

  it("wants every action in the set, not one of them", () => {
    expect(roleAllows("user", { map: ["import", "rank"] })).toBe(false);
    expect(roleAllows("moderator", { map: ["import", "rank"] })).toBe(true);
  });

  it("adds the roles in a column that holds more than one", () => {
    expect(roleAllows("user,moderator", { map: ["rank"] })).toBe(true);
    expect(roleAllows("moderator, user", { user: ["ban"] })).toBe(true);
  });

  it("refuses a role that is not in the vocabulary", () => {
    expect(roleAllows("owner", { map: ["import"] })).toBe(false);
    expect(roleAllows("", { map: ["import"] })).toBe(false);
  });
});

describe("isRoleName", () => {
  it("takes every name the vocabulary holds and nothing else", () => {
    for (const name of ROLE_NAMES) expect(isRoleName(name)).toBe(true);
    expect(isRoleName("root")).toBe(false);
  });
});

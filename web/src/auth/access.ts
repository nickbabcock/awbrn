import { createAccessControl } from "better-auth/plugins/access";
import { adminAc, defaultStatements } from "better-auth/plugins/admin/access";

/**
 * Every action a role can hold, grouped by what it acts on.
 *
 * The vocabulary is code and not data: a role is a set of these actions, and
 * the only thing the database keeps is which role a user holds. Put an action
 * here before a server function asks for it.
 *
 * `user` and `session` come from the admin plugin and gate the endpoints it
 * adds. The two below gate this application.
 */
export const statement = {
  ...defaultStatements,
  map: ["import", "tag", "rank", "edit-any"],
  match: ["void", "view-any"],
} as const;

export const ac = createAccessControl(statement);

/**
 * What a signed-in player holds.
 *
 * `map:tag` is here because an author tags their own map. It does not say
 * which map: that is what `mapTagGrant` decides, and `map:edit-any` is the
 * action that reaches past ownership.
 */
const userRole = ac.newRole({
  user: [],
  session: [],
  map: ["import", "tag"],
  match: [],
});

/** What a moderator adds: curation of the catalog, and the abuse tools. */
const moderatorRole = ac.newRole({
  user: ["list", "get", "ban"],
  session: ["list", "revoke"],
  map: ["import", "tag", "rank", "edit-any"],
  match: ["void", "view-any"],
});

/** Everything a moderator holds, plus the rest of the admin plugin. */
const adminRole = ac.newRole({
  ...adminAc.statements,
  map: ["import", "tag", "rank", "edit-any"],
  match: ["void", "view-any"],
});

export const ROLES = {
  user: userRole,
  moderator: moderatorRole,
  admin: adminRole,
} as const;

export const ROLE_NAMES = ["user", "moderator", "admin"] as const;

export type RoleName = (typeof ROLE_NAMES)[number];

/** The role a user holds when the column is empty. */
export const DEFAULT_ROLE: RoleName = "user";

/**
 * The roles that reach the endpoints the admin plugin adds.
 *
 * A role left out of this list still passes the checks this application
 * makes; it only fails the plugin's own routes, such as listing users.
 */
export const ADMIN_ROLES: RoleName[] = ["admin", "moderator"];

/** A set of actions to ask about, such as `{ map: ["rank"] }`. */
export type Permissions = {
  readonly [Resource in keyof typeof statement]?: readonly (typeof statement)[Resource][number][];
};

export function isRoleName(value: string): value is RoleName {
  return (ROLE_NAMES as readonly string[]).includes(value);
}

/**
 * Whether a role column allows every action named here.
 *
 * The column holds a comma separated list, which is the shape the admin
 * plugin writes and reads. A user holding more than one role passes when any
 * one of them passes, so roles add and never subtract.
 */
export function roleAllows(role: string | null | undefined, permissions: Permissions): boolean {
  const held = (role ?? DEFAULT_ROLE).split(",");
  return held.some((name) => {
    const trimmed = name.trim();
    return isRoleName(trimmed) && ROLES[trimmed].authorize(permissions).success;
  });
}

import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { getAuth } from "./auth.server.ts";

let accounts = 0;

/** A new account, and the headers of its signed-in session. */
async function signUp(name: string) {
  const email = `player-${++accounts}-${crypto.randomUUID()}@example.com`;
  const result = await getAuth().api.signUpEmail({
    body: { email, name, password: "correct-horse-battery" },
    returnHeaders: true,
  });
  const cookie = result.headers.getSetCookie().map((line) => line.split(";")[0]);
  return {
    userId: result.response.user.id,
    headers: new Headers({ cookie: cookie.join("; ") }),
  };
}

function rename(headers: Headers, name: string) {
  return getAuth().api.updateUser({ body: { name }, headers });
}

describe("getAuth", () => {
  // Better Auth rejects every request when the Drizzle schema has a column it
  // does not expect, so this guards schema drift across Better Auth upgrades.
  it("signs up and signs in against the migrated schema", async () => {
    const auth = getAuth();
    const credentials = { email: "player@example.com", password: "correct-horse-battery" };

    const signUp = await auth.api.signUpEmail({ body: { ...credentials, name: "player" } });
    const signIn = await auth.api.signInEmail({ body: credentials });

    expect(signIn.user.id).toBe(signUp.user.id);
  });
});

describe("display names", () => {
  it("stores the name normalized at sign-up", async () => {
    const { userId } = await signUp("  Olaf   Fan ");
    const row = await env.DB.prepare("SELECT name, nameKey FROM user WHERE id = ?")
      .bind(userId)
      .first();
    expect(row).toEqual({ name: "Olaf Fan", nameKey: "olaffan" });
  });

  it("turns away a name another player reads as", async () => {
    await signUp("Sonja Prime");
    await expect(signUp("sonja_prime")).rejects.toThrow("That name is taken");
  });

  it("turns away a reserved name the client never checked", async () => {
    await expect(signUp("Moderator")).rejects.toThrow("That name is reserved");
  });

  it("lets a player rename once, then holds the next rename", async () => {
    const { userId, headers } = await signUp("Eagle One");

    await rename(headers, "Eagle Two");
    await expect(rename(headers, "Eagle Three")).rejects.toThrow("once every 30 days");

    const row = await env.DB.prepare("SELECT name FROM user WHERE id = ?").bind(userId).first();
    expect(row).toEqual({ name: "Eagle Two" });
  });

  it("lets a player send their current name again during the cooldown", async () => {
    const { headers } = await signUp("drake fan");
    await rename(headers, "Drake Fan");
    await expect(rename(headers, "Drake Fan")).resolves.toBeTruthy();
  });

  it("turns away a rename onto a name already held", async () => {
    await signUp("Jess Prime");
    const { headers } = await signUp("Jess Second");
    await expect(rename(headers, "JESS PRIME")).rejects.toThrow("That name is taken");
  });
});

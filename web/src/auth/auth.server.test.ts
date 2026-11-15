import { describe, expect, it } from "vitest";
import { getAuth } from "./auth.server.ts";

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

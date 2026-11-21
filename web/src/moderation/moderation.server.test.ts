import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { actorFromRole } from "#/auth/actor.ts";
import { getAuth } from "#/auth/auth.server.ts";
import { renameUser } from "./moderation.server.ts";

async function signUp(name: string): Promise<string> {
  const { user } = await getAuth().api.signUpEmail({
    body: { email: `${crypto.randomUUID()}@example.com`, name, password: "correct-horse-battery" },
  });
  return user.id;
}

describe("renameUser", () => {
  it("renames, records the old name, and leaves the player's own rename unspent", async () => {
    const moderatorId = await signUp("Rename Moderator");
    const playerId = await signUp("Rude Name");

    const result = await renameUser({
      actor: actorFromRole(moderatorId, "moderator"),
      userId: playerId,
      name: "Player 1",
      reason: "Offensive name",
    });
    expect(result).toEqual({ ok: true, name: "Player 1" });

    const player = await env.DB.prepare(
      "SELECT name, nameKey, nameChangedAt FROM user WHERE id = ?",
    )
      .bind(playerId)
      .first();
    expect(player).toEqual({ name: "Player 1", nameKey: "player1", nameChangedAt: null });

    const logged = await env.DB.prepare(
      "SELECT action, details FROM moderation_actions WHERE subjectId = ?",
    )
      .bind(playerId)
      .first<{ action: string; details: string }>();
    expect(logged?.action).toBe("user.rename");
    expect(JSON.parse(logged?.details ?? "null")).toEqual({ from: "Rude Name", to: "Player 1" });
  });

  it("will not hand a player a name someone else holds", async () => {
    const moderatorId = await signUp("Second Moderator");
    await signUp("Held Name");
    const playerId = await signUp("Other Name");

    const result = await renameUser({
      actor: actorFromRole(moderatorId, "moderator"),
      userId: playerId,
      name: "held.name",
      reason: "Testing",
    });
    expect(result).toEqual({ ok: false, error: { message: "That name is taken" } });
  });
});

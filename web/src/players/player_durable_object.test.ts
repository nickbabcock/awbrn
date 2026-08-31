import { env } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getPlayerStubFrom } from "./player_service.ts";
import type { PlayerDurableObject } from "./player_durable_object.ts";

/**
 * A player object nobody else's test has touched.
 *
 * The objects are named after a player, so a fresh name is a fresh object and
 * there is no state to tear down between these.
 */
function freshPlayer(): DurableObjectStub<PlayerDurableObject> {
  return getPlayerStubFrom(env.PLAYERS, `test-${crypto.randomUUID()}`);
}

const subscription = {
  endpoint: "https://push.example.com/send/abc",
  // A real P-256 point, because the delivery path derives a key from it.
  p256dh: "BEl62iUYgUivxIkv69yViEuiBIa-Ib9-SkvMeAtA3LFgDzkrxZJjSgSnfckjBJuBkr3qBUYIHBQFLXYp5Nksh8U",
  auth: "tBHItJI5svbpez7KI4CCXg",
  label: null,
};

describe("PlayerDurableObject", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("keeps a browser's subscription and gives it up again", async () => {
    const player = freshPlayer();

    expect(await player.hasPushSubscription()).toBe(false);
    await player.addPushSubscription(subscription);
    expect(await player.hasPushSubscription()).toBe(true);

    // Subscribing twice is the same browser with rotated keys, not a second.
    await player.addPushSubscription({ ...subscription, auth: "AAHItJI5svbpez7KI4CCXg" });
    expect(await player.hasPushSubscription()).toBe(true);

    await player.removePushSubscription(subscription.endpoint);
    expect(await player.hasPushSubscription()).toBe(false);
  });

  it("collects the turns of a player with nothing open and sets one alarm", async () => {
    const player = freshPlayer();
    await player.addPushSubscription(subscription);

    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island",
      deadlineAt: Date.now() + 86_400_000,
    });
    await player.notify({
      type: "turnStarted",
      matchId: "m2",
      matchName: "Duo Falls",
      deadlineAt: null,
    });

    await runInDurableObject(player, async (_instance, state) => {
      const pending = state.storage.sql
        .exec("SELECT matchId FROM pending_turns ORDER BY matchId")
        .toArray();
      expect(pending.map((row) => row.matchId)).toEqual(["m1", "m2"]);
      // Both turns join one alarm, which is what makes them one notification.
      expect(await state.storage.getAlarm()).not.toBeNull();
    });
  });

  it("counts a match that moves twice before the alarm only once", async () => {
    const player = freshPlayer();
    await player.addPushSubscription(subscription);

    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island",
      deadlineAt: null,
    });
    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island renamed",
      deadlineAt: null,
    });

    await runInDurableObject(player, async (_instance, state) => {
      const pending = state.storage.sql.exec("SELECT matchName FROM pending_turns").toArray();
      expect(pending).toHaveLength(1);
      expect(pending[0]!.matchName).toBe("Sand Island renamed");
    });
  });

  it("never collects a turn that merely ended", async () => {
    const player = freshPlayer();
    await player.addPushSubscription(subscription);

    await player.notify({ type: "turnEnded", matchId: "m1" });

    await runInDurableObject(player, async (_instance, state) => {
      expect(state.storage.sql.exec("SELECT matchId FROM pending_turns").toArray()).toHaveLength(0);
      expect(await state.storage.getAlarm()).toBeNull();
    });
  });

  it("collects nothing at all for a player no notification could reach", async () => {
    // No browser has asked for notifications, so there is nothing to collect
    // for and no reason to wake this object ten seconds later to find out.
    const player = freshPlayer();
    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island",
      deadlineAt: null,
    });

    await runInDurableObject(player, async (_instance, state) => {
      expect(state.storage.sql.exec("SELECT matchId FROM pending_turns").toArray()).toHaveLength(0);
      expect(await state.storage.getAlarm()).toBeNull();
    });
  });

  it("drops what it collected when the browser gives up in the meantime", async () => {
    const player = freshPlayer();
    await player.addPushSubscription(subscription);
    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island",
      deadlineAt: null,
    });
    // The player turns notifications off before the alarm gets to send.
    await player.removePushSubscription(subscription.endpoint);

    await runInDurableObject(player, async (instance, state) => {
      await instance.alarm!();
      // Holding it would announce a turn long since played the first time a
      // browser subscribed again.
      expect(state.storage.sql.exec("SELECT matchId FROM pending_turns").toArray()).toHaveLength(0);
    });
  });

  it("forgets a subscription the push service says the browser dropped", async () => {
    const player = freshPlayer();
    await player.addPushSubscription(subscription);
    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island",
      deadlineAt: null,
    });

    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response(null, { status: 410 }));

    await runInDurableObject(player, async (instance) => {
      await instance.alarm!();
    });

    expect(fetchSpy).toHaveBeenCalledOnce();
    expect(await player.hasPushSubscription()).toBe(false);
  });

  it("keeps a turn that arrives while a notification is being sent", async () => {
    const player = freshPlayer();
    await player.addPushSubscription(subscription);
    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island",
      deadlineAt: null,
    });

    await runInDurableObject(player, async (instance, state) => {
      vi.spyOn(globalThis, "fetch").mockImplementation(async () => {
        // A second match moves while the first notification is in flight. Its
        // turn must survive the cleanup of the batch it arrived behind, or
        // nothing is left to announce it and the player never hears.
        await instance.notify({
          type: "turnStarted",
          matchId: "m2",
          matchName: "Duo Falls",
          deadlineAt: null,
        });
        return new Response(null, { status: 201 });
      });

      await instance.alarm!();

      const rows = state.storage.sql.exec("SELECT matchId FROM pending_turns").toArray();
      expect(rows.map((row) => row.matchId)).toEqual(["m2"]);
    });
  });

  it("holds the turns and tries again when a push service is merely unwell", async () => {
    const player = freshPlayer();
    await player.addPushSubscription(subscription);
    await player.notify({
      type: "turnStarted",
      matchId: "m1",
      matchName: "Sand Island",
      deadlineAt: null,
    });

    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(null, { status: 503 }));

    await runInDurableObject(player, async (instance, state) => {
      await instance.alarm!();
      // The turn is still owed to the player, so it stays with a later alarm.
      expect(state.storage.sql.exec("SELECT matchId FROM pending_turns").toArray()).toHaveLength(1);
      expect(await state.storage.getAlarm()).not.toBeNull();
    });
    // A service having a bad minute does not cost the browser its place.
    expect(await player.hasPushSubscription()).toBe(true);
  });
});

import { env } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { afterEach, describe, expect, it, vi } from "vitest";
import map178597 from "../../../assets/maps/178597.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { awbrnMapDocumentSchema } from "#/maps/map_document.ts";
import { importAwbwMapDocument, initSync, WasmMatch } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";
import type { MatchDurableObject } from "./match_durable_object.ts";
import { defaultMatchClock, type MatchSetup } from "./schemas.ts";

initSync({ module: serverWasmModule });

/** A match the server opens on its own seat's turn, against one person. */
function serverFirstSetup(): MatchSetup {
  const { document } = importAwbwMapDocument(awbwMapDataSchema.parse(map178597));
  return {
    matchId: "aifailure0001",
    mapId: "000000178597",
    revision: 1,
    map: awbrnMapDocumentSchema.parse(document),
    players: [
      {
        userId: null,
        aiProfileId: "ai-easy-v2",
        factionId: 1,
        team: null,
        startingFunds: 10_000,
        coId: 1,
      },
      {
        userId: "user_1",
        factionId: 2,
        team: null,
        startingFunds: 10_000,
        coId: 2,
      },
    ],
    fogEnabled: false,
    startingFunds: 10_000,
    clock: defaultMatchClock,
    creatorUserId: "user_1",
  };
}

/** Every reading of the match a seat or a watcher can be sent. */
function readings(game: WasmMatch) {
  return {
    seats: [game.playerGameState(0), game.playerGameState(1)],
    spectator: game.spectatorGameState(),
    pendingAiSlot: game.pendingAiSlot(),
  };
}

/**
 * Settle as the alarm does, and report both failure counts and how far ahead
 * the next wake is.
 */
async function settleOnAlarm(
  instance: MatchDurableObject,
  state: DurableObjectState,
  setup: MatchSetup,
): Promise<{ failures: unknown; turnFailures: unknown; waitMs: [number, number] }> {
  const before = Date.now();
  await instance["settle"](setup, { fromAlarm: true, retryCount: 0 });
  const after = Date.now();
  const alarm = await state.storage.getAlarm();
  expect(alarm).not.toBeNull();
  return {
    failures: await state.storage.get("settleFailures"),
    turnFailures: (await state.storage.get<{ failures: number }>("aiTurnFailures"))?.failures,
    waitMs: [alarm! - after, alarm! - before],
  };
}

/** Fake `Date` a year ahead, so no alarm comes due on its own and each test runs its wakes. */
function holdAlarms() {
  vi.useFakeTimers({ toFake: ["Date"], now: Date.now() + 365 * 24 * 60 * 60_000 });
}

describe("a server seat whose turn fails", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("leaves the match as its log holds it, and able to play the turn again", async () => {
    const setup = serverFirstSetup();
    const stub = env.MATCHES.get(env.MATCHES.idFromName(`test-${crypto.randomUUID()}`));

    await runInDurableObject(stub, async (instance) => {
      instance["appendEvent"]({ kind: "setup", payload: setup });
      const game = instance["loadGame"]();
      expect(game).not.toBeNull();

      // The turn gets a command taken before it traps, and the trap leaves the
      // object it ran on unusable, as a panic in the engine does.
      const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementationOnce(function (
        this: WasmMatch,
        slot: number,
      ) {
        this.process_action(slot, { type: "endTurn" });
        this.free();
        throw new WebAssembly.RuntimeError("unreachable");
      });

      expect(await instance["playPendingAiTurn"](setup, game!)).toBe(false);

      expect(runAiTurn).toHaveBeenCalledOnce();
      expect(instance["readActionEvents"]()).toEqual([]);
      const restored = instance["loadGame"]();
      expect(restored).not.toBeNull();
      expect(restored!.pendingAiSlot()).toBe(0);
      expect(readings(restored!)).toEqual(readings(new WasmMatch(setup)));

      // The turn is still owed, and the next wake plays it.
      expect(await instance["playPendingAiTurn"](setup, restored!)).toBe(true);

      const events = instance["readActionEvents"]();
      expect(events.at(-1)?.command).toEqual({ type: "endTurn" });
      expect(instance["loadGame"]()!.pendingAiSlot()).toBeUndefined();
      expect(WasmMatch.reconstructFromEvents(setup, events).pendingAiSlot()).toBeUndefined();
    });
  });

  it("is asked again less often each time it fails, and as before once it plays", async () => {
    const setup = serverFirstSetup();
    const stub = env.MATCHES.get(env.MATCHES.idFromName(`test-${crypto.randomUUID()}`));

    await runInDurableObject(stub, async (instance, state) => {
      instance["appendEvent"]({ kind: "setup", payload: setup });
      const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementation(() => {
        throw new WebAssembly.RuntimeError("unreachable");
      });

      // Each wake asks for the turn once, and the wait doubles from the
      // platform's own two seconds.
      for (const [failures, delayMs] of [
        [1, 2_000],
        [2, 4_000],
        [3, 8_000],
        [4, 16_000],
      ]) {
        const settled = await settleOnAlarm(instance, state, setup);
        expect(runAiTurn).toHaveBeenCalledTimes(failures);
        expect(settled.failures).toBe(failures);
        expect(settled.turnFailures).toBe(failures);
        expect(settled.waitMs[0]).toBeLessThanOrEqual(delayMs);
        expect(settled.waitMs[1]).toBeGreaterThanOrEqual(delayMs);
      }

      // The wait stops growing at five minutes, and the seat's clock, a week
      // away, is still what ends a match whose seat never plays.
      const counted = await state.storage.get<{ failures: number }>("aiTurnFailures");
      await state.storage.put("aiTurnFailures", { ...counted, failures: 30 });
      const capped = await settleOnAlarm(instance, state, setup);
      expect(capped.turnFailures).toBe(31);
      expect(capped.waitMs[0]).toBeLessThanOrEqual(5 * 60_000);
      expect(capped.waitMs[1]).toBeGreaterThanOrEqual(5 * 60_000);

      // A turn that plays clears the count, and the match sleeps until the
      // person's deadline.
      runAiTurn.mockRestore();
      const played = await settleOnAlarm(instance, state, setup);
      expect(played.failures).toBeUndefined();
      expect(played.turnFailures).toBeUndefined();
      expect(instance["loadGame"]()!.pendingAiSlot()).toBeUndefined();
      expect(played.waitMs[0]).toBeGreaterThan(24 * 60 * 60_000);
    });
  });

  it("keeps a match it cannot read back on the backoff until it can", async () => {
    const setup = serverFirstSetup();
    const stub = env.MATCHES.get(env.MATCHES.idFromName(`test-${crypto.randomUUID()}`));

    await runInDurableObject(stub, async (instance, state) => {
      instance["appendEvent"]({ kind: "setup", payload: setup });
      expect(instance["loadGame"]()).not.toBeNull();

      // The turn traps, and the trap leaves the wasm instance unable to read
      // any match back from its log.
      const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementation(() => {
        throw new WebAssembly.RuntimeError("unreachable");
      });
      const reconstruct = vi.spyOn(WasmMatch, "reconstructFromEvents").mockImplementation(() => {
        throw new WebAssembly.RuntimeError("unreachable");
      });

      // The first wake fails on the turn and the rest on the rebuild, and
      // every one of them leaves the match on the backoff.
      for (const [failures, delayMs] of [
        [1, 2_000],
        [2, 4_000],
        [3, 8_000],
      ]) {
        const settled = await settleOnAlarm(instance, state, setup);
        expect(runAiTurn).toHaveBeenCalledOnce();
        expect(instance["loadGame"]()).toBeNull();
        expect(settled.failures).toBe(failures);
        expect(settled.waitMs[0]).toBeLessThanOrEqual(delayMs);
        expect(settled.waitMs[1]).toBeGreaterThanOrEqual(delayMs);
      }

      // An alarm set outside a settle, as after a person's command, is not
      // dropped either.
      await state.storage.deleteAlarm();
      await instance["armAlarm"](setup);
      expect(await state.storage.getAlarm()).not.toBeNull();

      // Once the match reads back and the turn plays, the count clears and
      // the match sleeps until the person's deadline.
      reconstruct.mockRestore();
      runAiTurn.mockRestore();
      const recovered = await settleOnAlarm(instance, state, setup);
      expect(recovered.failures).toBeUndefined();
      expect(recovered.turnFailures).toBeUndefined();
      expect(instance["loadGame"]()!.pendingAiSlot()).toBeUndefined();
      expect(recovered.waitMs[0]).toBeGreaterThan(24 * 60 * 60_000);
    });
  });

  it("is asked again on the backoff when it ends its turn still on the move", async () => {
    holdAlarms();
    const setup = serverFirstSetup();
    const stub = env.MATCHES.get(env.MATCHES.idFromName(`test-${crypto.randomUUID()}`));

    await runInDurableObject(stub, async (instance, state) => {
      instance["appendEvent"]({ kind: "setup", payload: setup });
      const runAiTurn = vi
        .spyOn(WasmMatch.prototype, "runAiTurn")
        .mockImplementationOnce(() => ({ actions: [] }));

      // An unrelated earlier failure, which the owed turn must not wait on.
      await state.storage.put("settleFailures", 5);
      await instance.alarm();
      expect(runAiTurn).toHaveBeenCalledOnce();
      expect(instance["loadGame"]()!.pendingAiSlot()).toBe(0);
      expect(await state.storage.get("settleFailures")).toBe(6);
      expect(await state.storage.get("aiTurnFailures")).toMatchObject({ slot: 0, failures: 1 });
      expect(await state.storage.getAlarm()).toBe(Date.now() + 2_000);

      await instance.alarm();
      expect(instance["loadGame"]()!.pendingAiSlot()).toBeUndefined();
      expect(await state.storage.get("settleFailures")).toBeUndefined();
      expect(await state.storage.get("aiTurnFailures")).toBeUndefined();
      expect(await state.storage.getAlarm()).toBeGreaterThan(Date.now() + 24 * 60 * 60_000);
    });
  });

  it("is asked again on the backoff when its turn cannot be written down", async () => {
    holdAlarms();
    const setup = serverFirstSetup();
    const stub = env.MATCHES.get(env.MATCHES.idFromName(`test-${crypto.randomUUID()}`));

    await runInDurableObject(stub, async (instance, state) => {
      instance["appendEvent"]({ kind: "setup", payload: setup });
      const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn");
      // As a full database would.
      const append = vi.spyOn(
        instance as unknown as { appendEvent(event: unknown): void },
        "appendEvent",
      );
      append.mockImplementation(() => {
        throw new Error("SQLITE_FULL");
      });

      await instance.alarm();
      expect(runAiTurn).toHaveBeenCalledOnce();
      expect(instance["readActionEvents"]()).toEqual([]);
      expect(instance["loadGame"]()!.pendingAiSlot()).toBe(0);
      expect(await state.storage.get("aiTurnFailures")).toMatchObject({ slot: 0, failures: 1 });
      expect(await state.storage.getAlarm()).toBe(Date.now() + 2_000);

      await instance.alarm();
      expect(runAiTurn).toHaveBeenCalledTimes(2);
      expect(await state.storage.get("aiTurnFailures")).toMatchObject({ slot: 0, failures: 2 });
      expect(await state.storage.getAlarm()).toBe(Date.now() + 4_000);

      append.mockRestore();
      await instance.alarm();
      expect(instance["loadGame"]()!.pendingAiSlot()).toBeUndefined();
      expect(await state.storage.get("aiTurnFailures")).toBeUndefined();
    });
  });

  it("keeps a person's command whose following server turn fails", async () => {
    holdAlarms();
    const setup = serverFirstSetup();
    const stub = env.MATCHES.get(env.MATCHES.idFromName(`test-${crypto.randomUUID()}`));

    await runInDurableObject(stub, async (instance, state) => {
      instance["appendEvent"]({ kind: "setup", payload: setup });
      // The server's opening turn plays, and the person is on the move.
      expect(await instance["playPendingAiTurn"](setup, instance["loadGame"]()!)).toBe(true);
      const before = instance["readActionEvents"]().length;

      const sent: { type?: unknown }[] = [];
      const socket = {
        deserializeAttachment: () => ({ userId: "user_1", slotIndex: 1 }),
        serializeAttachment: () => {},
        send: (message: string) => sent.push(JSON.parse(message)),
      } as unknown as WebSocket;
      vi.spyOn(instance["ctx"], "getWebSockets").mockReturnValue([socket]);
      const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementationOnce(() => {
        throw new WebAssembly.RuntimeError("unreachable");
      });

      await instance.webSocketMessage(socket, JSON.stringify({ type: "endTurn" }));

      expect(runAiTurn).not.toHaveBeenCalled();
      expect(instance["readActionEvents"]()).toHaveLength(before + 1);
      expect(sent.some((message) => message.type === "playerUpdate")).toBe(true);
      expect(await state.storage.getAlarm()).toBe(Date.now());

      // An unrelated earlier failure, which the owed turn must not wait on.
      await state.storage.put("settleFailures", 5);
      await instance.alarm();
      expect(runAiTurn).toHaveBeenCalledOnce();
      const events = instance["readActionEvents"]();
      expect(events).toHaveLength(before + 1);
      expect(events.at(-1)).toMatchObject({ player: 1, command: { type: "endTurn" } });
      expect(sent.some((message) => message.type === "error")).toBe(false);
      expect(instance["loadGame"]()!.pendingAiSlot()).toBe(0);
      expect(await state.storage.get("settleFailures")).toBe(6);
      expect(await state.storage.get("aiTurnFailures")).toMatchObject({ slot: 0, failures: 1 });
      expect(await state.storage.getAlarm()).toBe(Date.now() + 2_000);
    });
  });
});

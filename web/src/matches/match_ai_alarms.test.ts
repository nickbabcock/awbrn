import { env } from "cloudflare:workers";
import { evictDurableObject, runDurableObjectAlarm, runInDurableObject } from "cloudflare:test";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import map162795 from "../../../assets/maps/162795.json";
import map178597 from "../../../assets/maps/178597.json";
import { getAuth } from "#/auth/auth.server.ts";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { awbrnMapDocumentSchema } from "#/maps/map_document.ts";
import { importAwbwMapDocument, initSync, WasmMatch } from "#/wasm/awbrn_server.js";
import type { StoredActionEvent } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";
import type { MatchDurableObject } from "./match_durable_object.ts";
import { defaultMatchClock, type MatchSetup } from "./schemas.ts";

initSync({ module: serverWasmModule });

const DAY_MS = 24 * 60 * 60_000;

const PERSON = "user_1";

/** A five-seat free-for-all with the person in `personSlot` and the server in every other seat. */
function tableSetup(personSlot: number | null, person = PERSON): MatchSetup {
  const { document } = importAwbwMapDocument(awbwMapDataSchema.parse(map162795));
  return {
    matchId: "aialarms00001",
    mapId: "000000162795",
    revision: 1,
    map: awbrnMapDocumentSchema.parse(document),
    players: [0, 1, 2, 3, 4].map((slot) => ({
      userId: slot === personSlot ? person : null,
      aiProfileId: slot === personSlot ? null : "ai-easy-v2",
      factionId: slot + 1,
      team: null,
      startingFunds: 10_000,
      coId: 1,
    })),
    fogEnabled: false,
    startingFunds: 10_000,
    clock: defaultMatchClock,
    creatorUserId: person,
  };
}

/** A duel the server opens against `person`, with the D1 rows a result needs. */
async function recordedDuel(person: string): Promise<MatchSetup> {
  const suffix = crypto.randomUUID().replace(/-/g, "").slice(0, 8);
  const matchId = `aiend${suffix}`;
  const mapId = `${suffix}0000`;
  const now = Math.floor(Date.now() / 1000);
  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO user (id, name, email, emailVerified, createdAt, updatedAt) VALUES (?, ?, ?, 1, ?, ?)",
    ).bind(person, "Player", `${person}@example.com`, now, now),
    env.DB.prepare(
      "INSERT INTO maps (id, name, author, currentRevision, createdAt, updatedAt) VALUES (?, 'Test', 'AWBW', 1, ?, ?)",
    ).bind(mapId, now, now),
    env.DB.prepare(`
      INSERT INTO map_revisions
        (mapId, revision, contentHash, width, height, playerCount, propertySignature, unitSignature, createdAt)
      VALUES (?, 1, 'hash', 27, 8, 2, 'p', 'u', ?)
    `).bind(mapId, now),
    env.DB.prepare(`
      INSERT INTO matches
        (id, name, phase, creatorUserId, mapId, mapRevision, maxPlayers, isPrivate, joinSlug, settings, createdAt, updatedAt)
      VALUES (?, 'Duel', 'active', ?, ?, 1, 2, 0, NULL, ?, ?, ?)
    `).bind(
      matchId,
      person,
      mapId,
      JSON.stringify({ fogEnabled: false, startingFunds: 10_000, clock: defaultMatchClock }),
      now,
      now,
    ),
  ]);

  const { document } = importAwbwMapDocument(awbwMapDataSchema.parse(map178597));
  return {
    matchId,
    mapId,
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
      { userId: person, factionId: 2, team: null, startingFunds: 10_000, coId: 2 },
    ],
    fogEnabled: false,
    startingFunds: 10_000,
    clock: defaultMatchClock,
    creatorUserId: person,
  };
}

/** Sign a person up, returning their id and session cookie. */
async function signUp(): Promise<{ userId: string; cookie: string }> {
  const email = `${crypto.randomUUID()}@example.com`;
  const { headers, response } = await getAuth().api.signUpEmail({
    body: { email, password: "correct-horse-battery", name: "player" },
    returnHeaders: true,
  });
  const cookie = headers
    .getSetCookie()
    .map((value) => value.split(";")[0])
    .join("; ");
  return { userId: response.user.id, cookie };
}

/** The turns the server's seats play back to back from `game`, up to a person's turn. */
function turnsInARow(game: WasmMatch, limit = Infinity): StoredActionEvent[][] {
  const turns: StoredActionEvent[][] = [];
  for (let slot = game.pendingAiSlot(); slot !== undefined && turns.length < limit;) {
    turns.push(game.runAiTurn(slot).actions.map((action) => action.storedActionEvent));
    slot = game.pendingAiSlot();
  }
  return turns;
}

interface Sent {
  type?: unknown;
  activePlayerSlot?: unknown;
}

interface Publisher {
  publishTurnState(setup: MatchSetup): Promise<void>;
}

/** A socket for the person in `slotIndex` that records what it is sent. */
function watch(instance: MatchDurableObject, slotIndex: number, person = PERSON) {
  const sent: Sent[] = [];
  const socket = {
    deserializeAttachment: () => ({ userId: person, slotIndex }),
    serializeAttachment: () => {},
    send: (message: string) => sent.push(JSON.parse(message)),
  } as unknown as WebSocket;
  vi.spyOn(instance["ctx"], "getWebSockets").mockReturnValue([socket]);
  return { socket, sent };
}

function playerUpdates(sent: Sent[]): Sent[] {
  return sent.filter((message) => message.type === "playerUpdate");
}

function newMatch() {
  return env.MATCHES.get(env.MATCHES.idFromName(`test-${crypto.randomUUID()}`));
}

describe("the server's seats", () => {
  beforeEach(() => {
    // A year ahead, so no alarm comes due on its own: the test runs each wake.
    vi.useFakeTimers({ toFake: ["Date"], now: Date.now() + 365 * DAY_MS });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("play after a person's command one wake at a time, each sent on as it plays", async () => {
    const setup = tableSetup(0);
    const reference = new WasmMatch(setup);
    const command = reference.process_action(0, { type: "endTurn" }).storedActionEvent;
    const turns = [[command], ...turnsInARow(reference)];
    expect(turns).toHaveLength(5);

    const stub = newMatch();
    const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn");
    let sent: Sent[] = [];

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(await instance.initializeMatch(setup)).toMatchObject({ ok: true });
      const watched = watch(instance, 0);
      sent = watched.sent;

      await instance.webSocketMessage(watched.socket, JSON.stringify({ type: "endTurn" }));

      expect(runAiTurn).not.toHaveBeenCalled();
      expect(instance["readActionEvents"]()).toEqual(turns[0]);
      expect(playerUpdates(sent).at(-1)?.activePlayerSlot).toBe(1);
      expect(sent.some((message) => message.type === "error")).toBe(false);
      expect(await state.storage.getAlarm()).toBe(Date.now());
    });

    for (const slot of [1, 2, 3, 4]) {
      sent.length = 0;
      expect(await runDurableObjectAlarm(stub)).toBe(true);

      await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
        expect(runAiTurn).toHaveBeenCalledTimes(slot);
        expect(runAiTurn).toHaveBeenLastCalledWith(slot);
        expect(instance["readActionEvents"]()).toEqual(turns.slice(0, slot + 1).flat());
        expect(playerUpdates(sent)).toHaveLength(turns[slot].length);
        expect(playerUpdates(sent).at(-1)?.activePlayerSlot).toBe((slot + 1) % 5);
        expect(sent.at(-1)?.type).toBe("clock");

        const alarm = await state.storage.getAlarm();
        if (slot < 4) {
          expect(alarm).toBe(Date.now());
        } else {
          expect(alarm).toBeGreaterThan(Date.now() + DAY_MS);
        }
      });
    }
  });

  it("keep the person's command and the turns before one that fails", async () => {
    const setup = tableSetup(0);
    const reference = new WasmMatch(setup);
    const command = reference.process_action(0, { type: "endTurn" }).storedActionEvent;
    const turns = [[command], ...turnsInARow(reference)];

    const stub = newMatch();
    let sent: Sent[] = [];
    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
      const watched = watch(instance, 0);
      sent = watched.sent;
      await instance.webSocketMessage(watched.socket, JSON.stringify({ type: "endTurn" }));
    });
    expect(await runDurableObjectAlarm(stub)).toBe(true);

    const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementationOnce(() => {
      throw new WebAssembly.RuntimeError("unreachable");
    });
    expect(await runDurableObjectAlarm(stub)).toBe(true);

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(runAiTurn).toHaveBeenCalledOnce();
      expect(instance["readActionEvents"]()).toEqual(turns.slice(0, 2).flat());
      expect(instance["loadGame"]()!.pendingAiSlot()).toBe(2);
      expect(sent.some((message) => message.type === "error")).toBe(false);
      expect(await state.storage.get("aiTurnFailures")).toMatchObject({ slot: 2, failures: 1 });
      expect(await state.storage.getAlarm()).toBe(Date.now() + 2_000);
    });

    expect(await runDurableObjectAlarm(stub)).toBe(true);

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(instance["readActionEvents"]()).toEqual(turns.slice(0, 3).flat());
      expect(await state.storage.get("aiTurnFailures")).toBeUndefined();
      expect(await state.storage.getAlarm()).toBe(Date.now());
    });
  });

  it("play each owed turn once, however many wakes and settles ask for it", async () => {
    const setup = tableSetup(0);
    const reference = new WasmMatch(setup);
    const command = reference.process_action(0, { type: "endTurn" }).storedActionEvent;
    const turns = [[command], ...turnsInARow(reference)];

    const stub = newMatch();
    const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn");

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      await instance.initializeMatch(setup);
      const { socket } = watch(instance, 0);
      await instance.webSocketMessage(socket, JSON.stringify({ type: "endTurn" }));

      // A wake delivered twice.
      await instance.alarm();
      await instance.alarm();
      expect(runAiTurn.mock.calls).toEqual([[1], [2]]);
      expect(instance["readActionEvents"]()).toEqual(turns.slice(0, 3).flat());

      // A connect's settle beside a wake.
      await Promise.all([
        instance.alarm(),
        instance["settle"](setup, { fromAlarm: false, retryCount: 0 }),
      ]);
      expect(runAiTurn.mock.calls).toEqual([[1], [2], [3]]);
      expect(instance["readActionEvents"]()).toEqual(turns.slice(0, 4).flat());
      expect(await state.storage.getAlarm()).toBe(Date.now());

      await instance.alarm();
      expect(runAiTurn.mock.calls).toEqual([[1], [2], [3], [4]]);
      expect(instance["readActionEvents"]()).toEqual(turns.flat());

      await Promise.all([
        instance.alarm(),
        instance["settle"](setup, { fromAlarm: false, retryCount: 0 }),
      ]);
      expect(runAiTurn).toHaveBeenCalledTimes(4);
      expect(instance["readActionEvents"]()).toEqual(turns.flat());
      expect(await state.storage.getAlarm()).toBeGreaterThan(Date.now() + DAY_MS);
    });
  });

  it("open a match on a wake that plays the seats ahead of the person", async () => {
    const setup = tableSetup(2);
    const turns = turnsInARow(new WasmMatch(setup));
    expect(turns).toHaveLength(2);

    const stub = newMatch();
    const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn");

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(await instance.initializeMatch(setup)).toMatchObject({ ok: true });
      expect(runAiTurn).not.toHaveBeenCalled();
      expect(instance["readActionEvents"]()).toEqual([]);
      expect(await state.storage.getAlarm()).toBe(Date.now());
    });

    expect(await runDurableObjectAlarm(stub)).toBe(true);
    expect(await runDurableObjectAlarm(stub)).toBe(true);

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(runAiTurn.mock.calls).toEqual([[0], [1]]);
      expect(instance["readActionEvents"]()).toEqual(turns.flat());
      expect(instance["loadGame"]()!.pendingAiSlot()).toBeUndefined();
      expect(await state.storage.getAlarm()).toBeGreaterThan(Date.now() + DAY_MS);
    });
  });

  it("play the game they play in a row, a wake at a time and read back from the log", async () => {
    const setup = tableSetup(null);
    const TURNS = 10;
    const turns = turnsInARow(new WasmMatch(setup), TURNS);
    expect(turns).toHaveLength(TURNS);

    const stub = newMatch();
    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
    });

    for (let turn = 1; turn <= TURNS; turn += 1) {
      expect(await runDurableObjectAlarm(stub)).toBe(true);
      if (turn % 2 === 0) {
        await evictDurableObject(stub);
      }
    }

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(instance["readActionEvents"]()).toEqual(turns.flat());
      expect(await state.storage.getAlarm()).toBe(Date.now());
    });
  });

  it("are not held back by a publish to the global database that fails", async () => {
    const setup = tableSetup(0);
    const stub = newMatch();
    let publish = vi.fn<Publisher["publishTurnState"]>();

    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
      const { socket } = watch(instance, 0);
      publish = vi
        .spyOn(instance as unknown as Publisher, "publishTurnState")
        .mockRejectedValue(new Error("the global database is down"));
      await instance.webSocketMessage(socket, JSON.stringify({ type: "endTurn" }));
    });

    for (const slot of [1, 2, 3]) {
      expect(await runDurableObjectAlarm(stub)).toBe(true);
      await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
        expect(publish).toHaveBeenCalledTimes(1 + slot);
        expect(instance["loadGame"]()!.pendingAiSlot()).toBe(slot + 1);
        expect(await state.storage.get("settleFailures")).toBeUndefined();
        expect(await state.storage.getAlarm()).toBe(Date.now());
      });
    }

    // The last seat hands the move to the person, so this wake waits on its publish.
    expect(await runDurableObjectAlarm(stub)).toBe(true);
    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(publish).toHaveBeenCalledTimes(5);
      expect(instance["loadGame"]()!.pendingAiSlot()).toBeUndefined();
      expect(await state.storage.get("settleFailures")).toBe(1);
      expect(await state.storage.getAlarm()).toBe(Date.now() + 2_000);
      publish.mockRestore();
      publish = vi.spyOn(instance as unknown as Publisher, "publishTurnState");
    });

    expect(await runDurableObjectAlarm(stub)).toBe(true);
    await runInDurableObject(stub, async (_instance: MatchDurableObject, state) => {
      expect(publish).toHaveBeenCalledOnce();
      await expect(publish.mock.results[0]?.value).resolves.toBeUndefined();
      expect(await state.storage.get("turnPublished")).toMatchObject({ activeSlotIndex: 0 });
      expect(await state.storage.get("settleFailures")).toBeUndefined();
      expect(await state.storage.getAlarm()).toBeGreaterThan(Date.now() + DAY_MS);
    });
  });

  it("are left to the alarm when a person connects", async () => {
    const { userId, cookie } = await signUp();
    const setup = tableSetup(2, userId);
    const stub = newMatch();

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      await instance.initializeMatch(setup);
      // As if the platform gave up on the opening seat's wake.
      await state.storage.deleteAlarm();
    });

    const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn");
    const response = await stub.fetch("https://example.com/", {
      headers: { Upgrade: "websocket", Cookie: cookie },
    });
    expect(response.status).toBe(101);
    response.webSocket!.accept();

    // `vi.waitFor` moves the faked clock on, so the wake is due by now, not at now.
    await vi.waitFor(async () => {
      const alarm = await runInDurableObject(stub, (_instance, state) => state.storage.getAlarm());
      expect(alarm).not.toBeNull();
      expect(alarm).toBeLessThanOrEqual(Date.now());
    });
    expect(runAiTurn).not.toHaveBeenCalled();
    response.webSocket!.close();

    expect(await runDurableObjectAlarm(stub)).toBe(true);
    expect(runAiTurn.mock.calls).toEqual([[0]]);
  });

  it("end a match on a turn of their own, and the result is written once", async () => {
    const person = `aiend-${crypto.randomUUID()}`;
    const setup = await recordedDuel(person);
    const stub = newMatch();
    let sent: Sent[] = [];

    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
      sent = watch(instance, 1, person).sent;
    });

    vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementationOnce(function (
      this: WasmMatch,
      slot: number,
    ) {
      return { actions: [this.process_action(slot, { type: "resign" })] };
    });
    expect(await runDurableObjectAlarm(stub)).toBe(true);

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(instance["loadGame"]()!.matchResults()).toBeTruthy();
      expect(await state.storage.get("resultsRecorded")).toBe(true);
      expect(sent.filter((message) => message.type === "matchOver")).toHaveLength(1);
      expect(await state.storage.getAlarm()).toBeNull();

      await instance.alarm();
      expect(sent.filter((message) => message.type === "matchOver")).toHaveLength(1);
    });

    const { results } = await env.DB.prepare(
      "SELECT slotIndex, outcome FROM match_results WHERE matchId = ? ORDER BY slotIndex",
    )
      .bind(setup.matchId)
      .all();
    expect(results).toEqual([
      { slotIndex: 0, outcome: "loss" },
      { slotIndex: 1, outcome: "win" },
    ]);
    const match = await env.DB.prepare("SELECT phase FROM matches WHERE id = ?")
      .bind(setup.matchId)
      .first();
    expect(match).toEqual({ phase: "completed" });
  });

  it("stay on a failing seat's backoff when the person sends a command meanwhile", async () => {
    const setup = tableSetup(0);
    const stub = newMatch();
    let socket!: WebSocket;
    let sent: Sent[] = [];

    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
      ({ socket, sent } = watch(instance, 0));
      await instance.webSocketMessage(socket, JSON.stringify({ type: "endTurn" }));
    });
    vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementationOnce(() => {
      throw new WebAssembly.RuntimeError("unreachable");
    });
    expect(await runDurableObjectAlarm(stub)).toBe(true);

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(await state.storage.getAlarm()).toBe(Date.now() + 2_000);
      await instance.webSocketMessage(socket, JSON.stringify({ type: "resign" }));
      expect(instance["readActionEvents"]().at(-1)).toMatchObject({
        player: 0,
        command: { type: "resign" },
      });
      expect(sent.some((message) => message.type === "error")).toBe(false);
      expect(instance["loadGame"]()!.pendingAiSlot()).toBe(1);
      expect(await state.storage.getAlarm()).toBe(Date.now() + 2_000);
    });
  });

  it("are asked for at once while something else a settle does keeps failing", async () => {
    const setup = tableSetup(0);
    const stub = newMatch();
    const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn");

    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
      const { socket } = watch(instance, 0);
      await instance.webSocketMessage(socket, JSON.stringify({ type: "endTurn" }));
      vi.spyOn(
        instance as unknown as { enforceClock(): number[] },
        "enforceClock",
      ).mockImplementation(() => {
        throw new Error("the clock cannot be read");
      });
    });

    for (const slot of [1, 2]) {
      expect(await runDurableObjectAlarm(stub)).toBe(true);
      await runInDurableObject(stub, async (_instance: MatchDurableObject, state) => {
        expect(runAiTurn).toHaveBeenCalledTimes(slot);
        expect(await state.storage.get("settleFailures")).toBe(slot);
        expect(await state.storage.getAlarm()).toBe(Date.now());
      });
    }
  });

  it("do not hand a failing seat's backoff on to the next seat", async () => {
    const setup = tableSetup(1);
    const stub = newMatch();
    let socket!: WebSocket;

    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
      ({ socket } = watch(instance, 1));
    });

    const runAiTurn = vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementation(() => {
      throw new WebAssembly.RuntimeError("unreachable");
    });
    expect(await runDurableObjectAlarm(stub)).toBe(true);
    expect(await runDurableObjectAlarm(stub)).toBe(true);
    runAiTurn.mockRestore();

    await runInDurableObject(stub, async (instance: MatchDurableObject, state) => {
      expect(await state.storage.get("aiTurnFailures")).toMatchObject({ slot: 0, failures: 2 });
      expect(await state.storage.getAlarm()).toBe(Date.now() + 4_000);

      // Past the failing seat's deadline, so the person's command times it out.
      vi.setSystemTime(Date.now() + 8 * DAY_MS);
      await instance.webSocketMessage(socket, JSON.stringify({ type: "endTurn" }));
      const events = instance["readActionEvents"]();
      expect(events.at(-2)).toMatchObject({ player: 0, command: { type: "timeout" } });
      expect(events.at(-1)).toMatchObject({ player: 1, command: { type: "endTurn" } });
      expect(instance["loadGame"]()!.pendingAiSlot()).toBe(2);
      expect(await state.storage.getAlarm()).toBe(Date.now());
    });
  });
  it("charge each seat only its own thinking", async () => {
    const clock = { initialMs: DAY_MS, incrementMs: 0, maxBankMs: DAY_MS };
    const setup = { ...tableSetup(2), clock };
    const thinkingMs = [10 * 60_000, 20 * 60_000];
    // Model production, where `Date` is frozen while code runs: a wall clock that
    // `Date` reaches at wakes and timers. Whole seconds, as the log stores.
    const start = Math.ceil(Date.now() / 1000) * 1000;
    let wall = start;
    vi.setSystemTime(start);
    const runAiTurn = WasmMatch.prototype.runAiTurn;
    vi.spyOn(WasmMatch.prototype, "runAiTurn").mockImplementation(function (
      this: WasmMatch,
      slot: number,
    ) {
      wall += thinkingMs[slot];
      return runAiTurn.call(this, slot);
    });
    vi.spyOn(scheduler, "wait").mockImplementation(async () => {
      vi.setSystemTime(wall);
    });

    const stub = newMatch();
    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      await instance.initializeMatch(setup);
    });
    for (let seat = 0; seat < 2; seat += 1) {
      vi.setSystemTime(wall);
      expect(await runDurableObjectAlarm(stub)).toBe(true);
    }

    await runInDurableObject(stub, async (instance: MatchDurableObject) => {
      const game = instance["loadGame"]()!;
      expect(game.pendingAiSlot()).toBeUndefined();
      const state = instance["clockState"](setup, game)!;
      expect(state.banksMs[0]).toBe(DAY_MS - thinkingMs[0]);
      expect(state.banksMs[1]).toBe(DAY_MS - thinkingMs[1]);
      expect(state.activeSlot).toBe(2);
      expect(state.turnStartedAt).toBe(start + thinkingMs[0] + thinkingMs[1]);
      expect(state.deadlineAt).toBe(state.turnStartedAt + DAY_MS);
    });
  });
});

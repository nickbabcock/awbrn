import { describe, expect, it, vi } from "vitest";
import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type { ObservedTransition } from "#/wasm/awbrn_server.js";
import { GameRunner } from "./game_runner.ts";
import type { CanvasCourierSurface, CanvasCourierTransport } from "#/canvas_courier/index.ts";
import type { GameWorker } from "./worker_types.ts";
import type { GameEvent } from "#/wasm/awbrn_wasm.js";

type GameInstance = Awaited<ReturnType<GameWorker["createGame"]>>;

function surfaceFixture() {
  const runner = new GameRunner();
  const createGame = vi.fn<GameWorker["createGame"]>();
  const internals = runner as unknown as {
    game: GameInstance | undefined;
    createGamePromise: Promise<GameInstance> | undefined;
    startSurface(surface: CanvasCourierSurface, transport: CanvasCourierTransport): void;
    getWorker(): GameWorker;
    handleGameEvent(event: GameEvent): void;
  };
  vi.spyOn(internals, "getWorker").mockReturnValue({ createGame } as unknown as GameWorker);
  const surface = {
    canvas: {} as HTMLCanvasElement,
    offscreen: { width: 300, height: 150 } as OffscreenCanvas,
  };
  const inputConfig = { buffer: new SharedArrayBuffer(64 + 8 * 32), capacity: 8 };
  const transport = {
    currentSize: () => ({ width: 960, height: 640, scaleFactor: 2 }),
    inputConfig,
  } as CanvasCourierTransport;
  return { runner, internals, createGame, surface, transport };
}

describe("GameRunner surface lifecycle", () => {
  it("does not restore a game or process its events after disposal", async () => {
    const { runner, internals, createGame, surface, transport } = surfaceFixture();
    const handleEvent = vi.spyOn(internals, "handleGameEvent");
    let resolve!: (game: GameInstance) => void;
    createGame.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    internals.startSurface(surface, transport);
    const pending = internals.createGamePromise!;
    expect(surface.offscreen).toMatchObject({ width: 960, height: 640 });
    expect(createGame.mock.calls[0]![3]).toBe(transport.inputConfig);

    runner.dispose();
    createGame.mock.calls[0]![4]!({ type: "NewDay", day: 7 });
    resolve({} as GameInstance);
    await pending;

    expect(handleEvent).not.toHaveBeenCalled();
    expect(internals.game).toBeUndefined();
    expect(internals.createGamePromise).toBeUndefined();
  });

  it("keeps the new game when an earlier surface completes initialization", async () => {
    const { runner, internals, createGame, surface, transport } = surfaceFixture();
    let resolve!: (game: GameInstance) => void;
    createGame.mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    internals.startSurface(surface, transport);
    const earlier = internals.createGamePromise!;

    const currentGame = {} as GameInstance;
    createGame.mockResolvedValueOnce(currentGame);
    internals.startSurface({ ...surface, offscreen: {} as OffscreenCanvas }, transport);
    await internals.createGamePromise;
    resolve({} as GameInstance);
    await earlier;

    expect(internals.game).toBe(currentGame);
    expect(createGame).toHaveBeenCalledTimes(2);
    runner.dispose();
  });
});

describe("GameRunner live transitions", () => {
  it("applies updates received during the live baseline after the baseline", async () => {
    const game = {
      applyLiveTransition: vi.fn().mockResolvedValue(undefined),
      loadLiveMatch: vi.fn().mockResolvedValue(undefined),
    };
    const runner = new GameRunner();
    const internals = runner as unknown as {
      game: typeof game;
    };
    internals.game = game;

    const first = {} as ObservedTransition;
    const second = {} as ObservedTransition;
    await runner.applyLiveTransition(first);
    await runner.applyLiveTransition(second);
    await runner.loadLiveMatch({} as AwbrnMapDocument, [], {});

    expect(game.loadLiveMatch).toHaveBeenCalledOnce();
    expect(game.applyLiveTransition).toHaveBeenNthCalledWith(1, first);
    expect(game.applyLiveTransition).toHaveBeenNthCalledWith(2, second);
    runner.dispose();
  });
});

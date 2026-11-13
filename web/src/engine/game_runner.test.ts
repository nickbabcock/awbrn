import { describe, expect, it, vi } from "vitest";
import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type { ObservedTransition } from "#/wasm/awbrn_server.js";
import { GameRunner } from "./game_runner.ts";
import type { CanvasCourierSurface, CanvasCourierTransport } from "#/canvas_courier/index.ts";
import type { GameWorker } from "./worker_types.ts";
import type { GameEvent } from "#/wasm/awbrn_wasm.js";
import { useGameStore } from "./store.ts";

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

describe("GameRunner camera", () => {
  function cameraFixture() {
    const game = {
      zoomBy: vi.fn().mockResolvedValue(undefined),
      fitBoard: vi.fn().mockResolvedValue(undefined),
      zoomTo: vi.fn().mockResolvedValue(undefined),
      restoreView: vi.fn().mockResolvedValue(undefined),
      centerOn: vi.fn().mockResolvedValue(undefined),
      setClearColor: vi.fn().mockResolvedValue(undefined),
    };
    const runner = new GameRunner();
    const internals = runner as unknown as {
      game: typeof game;
      handleGameEvent(event: GameEvent): void;
    };
    internals.game = game;
    return { runner, internals, game };
  }

  it("forwards each camera command to the game", async () => {
    const { runner, game } = cameraFixture();

    await runner.zoomBy(1.25);
    await runner.zoomBy(0.8, { x: 40, y: 12 });
    await runner.fitBoard();
    await runner.zoomTo(2.5, { x: 3, y: 4 });
    await runner.restoreView();
    await runner.centerOn({ x: 7, y: 9 });
    await runner.centerOn({ x: 1, y: 2 }, false);
    await runner.setClearColor({ r: 10, g: 17, b: 40, a: 1 });

    expect(game.zoomBy).toHaveBeenNthCalledWith(1, 1.25, undefined);
    expect(game.zoomBy).toHaveBeenNthCalledWith(2, 0.8, { x: 40, y: 12 });
    expect(game.fitBoard).toHaveBeenCalledOnce();
    expect(game.zoomTo).toHaveBeenCalledWith(2.5, { x: 3, y: 4 });
    expect(game.restoreView).toHaveBeenCalledOnce();
    expect(game.centerOn).toHaveBeenNthCalledWith(1, { x: 7, y: 9 }, true);
    expect(game.centerOn).toHaveBeenNthCalledWith(2, { x: 1, y: 2 }, false);
    expect(game.setClearColor).toHaveBeenCalledWith({ r: 10, g: 17, b: 40, a: 1 });
    runner.dispose();
  });

  it("keeps the latest board view for the page to read", () => {
    const { runner, internals } = cameraFixture();
    useGameStore.getState().actions.reset();
    const view = {
      scale: 2,
      fitScale: 0.71,
      maxScale: 4,
      touchFloorScale: 2.5,
      view: { x: 4.5, y: -1, width: 12, height: 20 },
      map: { width: 20, height: 20 },
    };

    internals.handleGameEvent({ type: "BoardView", ...view });

    expect(useGameStore.getState().boardView).toEqual({ type: "BoardView", ...view });
    useGameStore.getState().actions.reset();
    expect(useGameStore.getState().boardView).toBeNull();
    runner.dispose();
  });
});

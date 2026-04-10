import { proxy, transfer, wrap } from "comlink";
import {
  CanvasCourierTransport,
  type CanvasCourierController,
  type CanvasCourierSurface,
  type CanvasSize,
} from "#/canvas_courier/index.ts";
import type { GameEvent } from "#/wasm/awbrn_wasm.js";
import { gameAssetConfig } from "./asset_manifest";
import { useGameStore } from "./store";
import type { GameWorker } from "./worker_types";

type GameInstance = Awaited<ReturnType<GameWorker["createGame"]>>;

export interface GameSurface extends CanvasCourierSurface {}

export class GameRunner implements CanvasCourierController {
  private activeSurface: GameSurface | undefined;
  private createGamePromise: Promise<GameInstance> | undefined;
  private game: GameInstance | undefined;
  private rawWorker: Worker | undefined;
  private surfaceVersion = 0;
  private readonly transport = new CanvasCourierTransport();
  private transferredCanvas: HTMLCanvasElement | undefined;
  private worker: GameWorker | undefined;

  attachSurface(surface: GameSurface): void {
    if (this.activeSurface?.canvas === surface.canvas) {
      this.activeSurface = surface;
      return;
    }

    const version = ++this.surfaceVersion;
    this.activeSurface = surface;

    const measuredSize = this.transport.measureSurface(surface);
    this.prepareCanvasForAttachment(surface, measuredSize);
    this.transport.attachSurface(surface);

    void this.ensureGame(surface, measuredSize).catch((error) => {
      if (version === this.surfaceVersion) {
        console.error("GameRunner failed to initialize:", error);
      }
    });
  }

  async loadReplay(file: File | FileSystemFileHandle): Promise<void> {
    const game = await this.requireGame();
    await game.newReplay(file);
  }

  async loadMapPreview(mapId: number): Promise<void> {
    const game = await this.requireGame();
    await game.loadMapPreview(mapId);
  }

  async setPlayerDisplayFaction(playerId: number, factionId: number | null): Promise<void> {
    const game = await this.requireGame();
    await game.setPlayerDisplayFaction(playerId, factionId);
  }

  dispose(): void {
    this.surfaceVersion += 1;
    this.activeSurface = undefined;
    this.transport.dispose();
    this.game = undefined;
    this.createGamePromise = undefined;
    this.transferredCanvas = undefined;
    this.worker = undefined;
    this.rawWorker?.terminate();
    this.rawWorker = undefined;
  }

  private async ensureGame(surface: GameSurface, size: CanvasSize): Promise<GameInstance> {
    if (this.game) {
      return this.game;
    }

    if (!this.createGamePromise) {
      this.assertCanvasTransferable(surface.canvas);
      this.transferredCanvas = surface.canvas;

      this.createGamePromise = this.getWorker()
        .createGame(
          transfer(surface.offscreen, [surface.offscreen]),
          size,
          gameAssetConfig,
          this.transport.inputConfig,
          proxy((event: GameEvent) => {
            this.handleGameEvent(event);
          }),
        )
        .then((game) => {
          this.game = game;
          return game;
        })
        .catch((error) => {
          this.createGamePromise = undefined;
          throw error;
        });
    }

    return this.createGamePromise;
  }

  private handleGameEvent(event: GameEvent): void {
    switch (event.type) {
      case "NewDay": {
        useGameStore.getState().actions.setCurrentDay(event.day);
        break;
      }
      case "MapDimensions": {
        break;
      }
      case "ReplayLoaded": {
        break;
      }
      case "PlayerRosterUpdated": {
        useGameStore.getState().actions.setPlayerRoster(event);
        useGameStore.getState().actions.setCurrentDay(event.day);
        break;
      }
      default: {
        break;
      }
    }
  }

  private prepareCanvasForAttachment(surface: GameSurface, size: CanvasSize): void {
    if (this.transferredCanvas === undefined) {
      this.applyInitialCanvasSize(surface.offscreen, size);
      return;
    }

    this.assertCanvasTransferable(surface.canvas);
  }

  private assertCanvasTransferable(canvas: HTMLCanvasElement): void {
    if (this.transferredCanvas && this.transferredCanvas !== canvas) {
      throw new Error(
        "GameRunner cannot attach a different canvas after transferring to OffscreenCanvas.",
      );
    }
  }

  private applyInitialCanvasSize(offscreen: OffscreenCanvas, size: CanvasSize): void {
    offscreen.width = size.width;
    offscreen.height = size.height;
  }

  private async requireGame(): Promise<GameInstance> {
    if (this.game) {
      return this.game;
    }

    if (this.createGamePromise) {
      return this.createGamePromise;
    }

    throw new Error("GameRunner is not initialized yet.");
  }

  private getWorker(): GameWorker {
    if (!this.worker) {
      this.rawWorker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
      this.worker = wrap<GameWorker>(this.rawWorker);
    }

    return this.worker;
  }
}

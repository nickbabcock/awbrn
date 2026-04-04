import { proxy, transfer, wrap } from "comlink";
import {
  CanvasCourierTransport,
  type CanvasCourierController,
  type CanvasCourierSurface,
  type LogicalCanvasSize,
} from "#/canvas_courier/index.ts";
import type { GameEvent } from "#/wasm/awbrn_wasm.js";
import { gameAssetConfig } from "./asset_manifest";
import { useGameStore } from "./store";
import type { GameWorker } from "./worker_types";

type GameInstance = Awaited<ReturnType<GameWorker["createGame"]>>;

export interface GameSurface extends CanvasCourierSurface {}

interface MapDimensions {
  width: number;
  height: number;
}

export class GameRunner implements CanvasCourierController {
  private activeSurface: GameSurface | undefined;
  private createGamePromise: Promise<GameInstance> | undefined;
  private disposeTimer: number | undefined;
  private game: GameInstance | undefined;
  private latestMapDimensions: MapDimensions | undefined;
  private rawWorker: Worker | undefined;
  private surfaceVersion = 0;
  private readonly transport = new CanvasCourierTransport();
  private transferredCanvas: HTMLCanvasElement | undefined;
  private worker: GameWorker | undefined;

  async attachCanvas(surface: GameSurface): Promise<void> {
    return this.attachSurface(surface);
  }

  async attachSurface(surface: GameSurface): Promise<void> {
    this.cancelScheduledDispose();

    const version = ++this.surfaceVersion;
    this.activeSurface = surface;

    const measuredSize = this.transport.measureSurface(surface);
    this.prepareCanvasForAttachment(surface, measuredSize);

    await this.ensureGame(surface, measuredSize);
    if (version !== this.surfaceVersion || this.activeSurface?.canvas !== surface.canvas) {
      return;
    }

    this.transport.attachSurface(surface);
    this.applyMapDimensions();
  }

  detachCanvas(canvas: HTMLCanvasElement): void {
    this.detachSurface(canvas);
  }

  detachSurface(canvas: HTMLCanvasElement): void {
    if (this.activeSurface?.canvas !== canvas) {
      return;
    }

    this.surfaceVersion += 1;
    this.transport.detachSurface(canvas);
    this.activeSurface = undefined;
  }

  scheduleDispose(): void {
    this.cancelScheduledDispose();
    this.disposeTimer = window.setTimeout(() => {
      this.disposeTimer = undefined;
      if (!this.activeSurface) {
        this.dispose();
      }
    }, 0);
  }

  async loadReplay(file: File | FileSystemFileHandle): Promise<void> {
    const game = await this.requireGame();
    await game.newReplay(file);
  }

  async loadMapPreview(mapId: number): Promise<void> {
    const game = await this.requireGame();
    await game.loadMapPreview(mapId);
  }

  dispose(): void {
    this.cancelScheduledDispose();
    this.surfaceVersion += 1;
    this.activeSurface = undefined;
    this.transport.dispose();
    this.game = undefined;
    this.createGamePromise = undefined;
    this.latestMapDimensions = undefined;
    this.transferredCanvas = undefined;
    this.worker = undefined;
    this.rawWorker?.terminate();
    this.rawWorker = undefined;
  }

  private async ensureGame(surface: GameSurface, size: LogicalCanvasSize): Promise<GameInstance> {
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
        this.latestMapDimensions = {
          width: event.width,
          height: event.height,
        };
        this.applyMapDimensions();
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

  private applyMapDimensions(): void {
    const container = this.activeSurface?.container;
    if (!container || !this.latestMapDimensions) {
      return;
    }

    container.style.width = `${this.latestMapDimensions.width}px`;
    container.style.height = `${this.latestMapDimensions.height}px`;
  }

  private prepareCanvasForAttachment(surface: GameSurface, size: LogicalCanvasSize): void {
    if (this.transferredCanvas === undefined) {
      this.applyInitialCanvasSize(surface.offscreen, surface.canvas, size);
      return;
    }

    this.assertCanvasTransferable(surface.canvas);
    this.transport.applyVisibleCanvasSize(surface.canvas, size);
  }

  private assertCanvasTransferable(canvas: HTMLCanvasElement): void {
    if (this.transferredCanvas && this.transferredCanvas !== canvas) {
      throw new Error(
        "GameRunner cannot attach a different canvas after transferring to OffscreenCanvas.",
      );
    }
  }

  private applyInitialCanvasSize(
    offscreen: OffscreenCanvas,
    canvas: HTMLCanvasElement,
    size: LogicalCanvasSize,
  ): void {
    offscreen.width = Math.floor(size.width * size.scaleFactor);
    offscreen.height = Math.floor(size.height * size.scaleFactor);
    this.transport.applyVisibleCanvasSize(canvas, size);
  }

  private cancelScheduledDispose() {
    if (this.disposeTimer !== undefined) {
      window.clearTimeout(this.disposeTimer);
      this.disposeTimer = undefined;
    }
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

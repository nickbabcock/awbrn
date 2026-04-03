import { proxy, transfer, wrap } from "comlink";
import { gameAssetConfig } from "./asset_manifest";
import type { GameWorker } from "./worker_types";
import type { GameEvent } from "../wasm/awbrn_wasm";
import { useGameStore } from "./store";

type GameInstance = Awaited<ReturnType<GameWorker["createGame"]>>;

export interface GameSurface {
  canvas: HTMLCanvasElement;
  container: HTMLElement;
}

interface LogicalCanvasSize {
  width: number;
  height: number;
  scaleFactor: number;
}

interface MapDimensions {
  width: number;
  height: number;
}

export class GameRunner {
  private activeSurface: GameSurface | undefined;
  private attachmentAbortController: AbortController | undefined;
  private transferredCanvas: HTMLCanvasElement | undefined;
  private createGamePromise: Promise<GameInstance> | undefined;
  private game: GameInstance | undefined;
  private logicalCanvasSize: LogicalCanvasSize | undefined;
  private latestMapDimensions: MapDimensions | undefined;
  private rawWorker: Worker | undefined;
  private resizeObserver: ResizeObserver | undefined;
  private surfaceVersion = 0;
  private worker: GameWorker | undefined;

  async attachCanvas(surface: GameSurface): Promise<void> {
    const version = ++this.surfaceVersion;
    this.activeSurface = surface;

    const measuredSize = this.measureSurface(surface);
    this.logicalCanvasSize = measuredSize;

    this.prepareCanvasForAttachment(surface.canvas, measuredSize);

    const game = await this.ensureGame(surface, measuredSize);
    if (version !== this.surfaceVersion || this.activeSurface?.canvas !== surface.canvas) {
      return;
    }

    this.bindSurface(surface, game);
  }

  detachCanvas(canvas: HTMLCanvasElement): void {
    if (this.activeSurface?.canvas !== canvas) {
      return;
    }

    this.surfaceVersion += 1;
    this.activeSurface = undefined;
    this.releaseSurfaceBindings();
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
    this.surfaceVersion += 1;
    this.activeSurface = undefined;
    this.releaseSurfaceBindings();
    this.game = undefined;
    this.createGamePromise = undefined;
    this.latestMapDimensions = undefined;
    this.logicalCanvasSize = undefined;
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
      const offscreenCanvas = surface.canvas.transferControlToOffscreen();
      this.transferredCanvas = surface.canvas;

      this.createGamePromise = this.getWorker()
        .createGame(
          transfer(offscreenCanvas, [offscreenCanvas]),
          size,
          gameAssetConfig,
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

  private prepareCanvasForAttachment(canvas: HTMLCanvasElement, size: LogicalCanvasSize): void {
    if (this.transferredCanvas === undefined) {
      this.applyInitialCanvasSize(canvas, size);
      return;
    }

    this.assertCanvasTransferable(canvas);
    this.applyVisibleCanvasSize(canvas, size);
  }

  private assertCanvasTransferable(canvas: HTMLCanvasElement): void {
    if (this.transferredCanvas && this.transferredCanvas !== canvas) {
      throw new Error(
        "GameRunner cannot attach a different canvas after transferring to OffscreenCanvas.",
      );
    }
  }

  private bindSurface(surface: GameSurface, game: GameInstance): void {
    this.releaseSurfaceBindings();
    this.activeSurface = surface;

    const abortController = new AbortController();
    const listenerOptions = { signal: abortController.signal };

    this.attachmentAbortController = abortController;
    this.syncSurfaceSize(surface, game);
    this.applyMapDimensions();

    const toLogicalCanvasCoordinates = (event: MouseEvent) => {
      const rect = surface.canvas.getBoundingClientRect();
      const size = this.logicalCanvasSize;
      if (!size || rect.width <= 0 || rect.height <= 0) {
        return null;
      }

      return {
        x: ((event.clientX - rect.left) / rect.width) * size.width,
        y: ((event.clientY - rect.top) / rect.height) * size.height,
      };
    };

    surface.canvas.addEventListener(
      "keydown",
      (event) => {
        game.handleKeyDown({
          key: event.key,
          keyCode: event.code,
          repeat: event.repeat,
        });
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "keyup",
      (event) => {
        game.handleKeyUp({
          key: event.key,
          keyCode: event.code,
          repeat: event.repeat,
        });
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "blur",
      () => {
        game.handleBlur();
      },
      listenerOptions,
    );

    document.addEventListener(
      "visibilitychange",
      () => {
        if (document.hidden) {
          game.pause();
          return;
        }

        game.resume();
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "mousemove",
      (event) => {
        const logicalPosition = toLogicalCanvasCoordinates(event);
        if (!logicalPosition) {
          return;
        }

        game.handlePointerMove(logicalPosition);
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "mousedown",
      (event) => {
        const logicalPosition = toLogicalCanvasCoordinates(event);
        if (!logicalPosition) {
          return;
        }

        surface.canvas.focus({ preventScroll: true });
        game.handlePointerDown({
          button: event.button,
          ...logicalPosition,
        });
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "mouseup",
      (event) => {
        const logicalPosition = toLogicalCanvasCoordinates(event);
        if (!logicalPosition) {
          return;
        }

        game.handlePointerUp({
          button: event.button,
          ...logicalPosition,
        });
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "mouseleave",
      () => {
        game.handlePointerLeave();
      },
      listenerOptions,
    );

    this.resizeObserver = new ResizeObserver(() => {
      this.syncSurfaceSize(surface, game);
    });
    this.resizeObserver.observe(surface.container);

    surface.canvas.focus({ preventScroll: true });
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

  private syncSurfaceSize(surface: GameSurface, game: GameInstance): void {
    const nextSize = this.measureSurface(surface);
    this.logicalCanvasSize = nextSize;
    this.applyVisibleCanvasSize(surface.canvas, nextSize);
    game.resize(nextSize);
  }

  private measureSurface(surface: GameSurface): LogicalCanvasSize {
    const bounds = surface.container.getBoundingClientRect();
    const fallbackWidth = surface.canvas.clientWidth || surface.canvas.width;
    const fallbackHeight = surface.canvas.clientHeight || surface.canvas.height;
    const width = bounds.width > 0 ? bounds.width : fallbackWidth;
    const height = bounds.height > 0 ? bounds.height : fallbackHeight;
    const scaleFactor = window.devicePixelRatio;

    return {
      width: this.snapToDevicePixel(width, scaleFactor).logical,
      height: this.snapToDevicePixel(height, scaleFactor).logical,
      scaleFactor,
    };
  }

  private applyInitialCanvasSize(canvas: HTMLCanvasElement, size: LogicalCanvasSize): void {
    canvas.width = Math.floor(size.width * size.scaleFactor);
    canvas.height = Math.floor(size.height * size.scaleFactor);
    this.applyVisibleCanvasSize(canvas, size);
  }

  private applyVisibleCanvasSize(canvas: HTMLCanvasElement, size: LogicalCanvasSize): void {
    canvas.style.width = `${size.width}px`;
    canvas.style.height = `${size.height}px`;
  }

  private releaseSurfaceBindings(): void {
    this.attachmentAbortController?.abort();
    this.attachmentAbortController = undefined;
    this.resizeObserver?.disconnect();
    this.resizeObserver = undefined;
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

  private snapToDevicePixel(size: number, ratio: number) {
    const physicalSize = Math.floor(size * ratio);
    return {
      logical: Math.floor(physicalSize / ratio),
    };
  }

  private getWorker(): GameWorker {
    if (!this.worker) {
      this.rawWorker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
      this.worker = wrap<GameWorker>(this.rawWorker);
    }

    return this.worker;
  }
}

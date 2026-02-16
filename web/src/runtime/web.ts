import { proxy, transfer, wrap } from "comlink";
import type { GameEvent } from "../game_events";
import type { GameWorker } from "../worker_types";
import type { GameRuntime, RuntimeBootstrapArgs } from "./types";

type RemoteGame = Awaited<ReturnType<GameWorker["createGame"]>>;

export class WebRuntime implements GameRuntime {
  private game: RemoteGame | undefined;
  private worker: Worker | undefined;
  private resizeObserver: ResizeObserver | undefined;
  private abortController = new AbortController();

  async bootstrap({
    canvas,
    container,
    onEvent,
  }: RuntimeBootstrapArgs): Promise<void> {
    if (!canvas) {
      throw new Error("Canvas is required for the web runtime");
    }

    const bounds = container.getBoundingClientRect();
    const snappedWidth = snapToDevicePixel(
      bounds.width,
      window.devicePixelRatio,
    );
    const snappedHeight = snapToDevicePixel(
      bounds.height,
      window.devicePixelRatio,
    );

    canvas.width = snappedWidth.physical;
    canvas.height = snappedHeight.physical;
    canvas.style.width = `${snappedWidth.logical}px`;
    canvas.style.height = `${snappedHeight.logical}px`;

    const offscreenCanvas = canvas.transferControlToOffscreen();
    const worker = new Worker(new URL("../worker.ts", import.meta.url), {
      type: "module",
    });

    this.worker = worker;
    const workerApi = wrap<GameWorker>(worker);

    this.game = await workerApi.createGame(
      transfer(offscreenCanvas, [offscreenCanvas]),
      {
        width: snappedWidth.logical,
        height: snappedHeight.logical,
        scaleFactor: window.devicePixelRatio,
      },
      proxy((event: GameEvent) => {
        onEvent(event);
      }),
    );

    this.resizeObserver = new ResizeObserver(() => {
      if (!this.game) {
        return;
      }

      const nextBounds = container.getBoundingClientRect();
      const width = snapToDevicePixel(
        nextBounds.width,
        window.devicePixelRatio,
      );
      const height = snapToDevicePixel(
        nextBounds.height,
        window.devicePixelRatio,
      );

      canvas.style.width = `${width.logical}px`;
      canvas.style.height = `${height.logical}px`;

      void this.game.resize({
        width: width.logical,
        height: height.logical,
      });
    });
    this.resizeObserver.observe(container);

    document.addEventListener(
      "keydown",
      (event) => {
        if (!this.game) {
          return;
        }

        void this.game.handleKeyDown({
          key: event.key,
          keyCode: event.code,
          repeat: event.repeat,
        });
      },
      { signal: this.abortController.signal },
    );

    document.addEventListener(
      "keyup",
      (event) => {
        if (!this.game) {
          return;
        }

        void this.game.handleKeyUp({
          key: event.key,
          keyCode: event.code,
          repeat: event.repeat,
        });
      },
      { signal: this.abortController.signal },
    );

    document.addEventListener(
      "visibilitychange",
      () => {
        if (!this.game) {
          return;
        }

        if (document.hidden) {
          void this.game.pause();
        } else {
          void this.game.resume();
        }
      },
      { signal: this.abortController.signal },
    );
  }

  async loadReplay(data: Uint8Array): Promise<void> {
    if (!this.game) {
      return;
    }

    await this.game.newReplayData(data);
  }

  dispose(): void {
    this.abortController.abort();
    this.abortController = new AbortController();
    this.resizeObserver?.disconnect();
    this.resizeObserver = undefined;

    if (this.worker) {
      this.worker.terminate();
      this.worker = undefined;
    }

    this.game = undefined;
  }
}

function snapToDevicePixel(
  size: number,
  ratio: number,
): {
  logical: number;
  physical: number;
} {
  const physical = Math.floor(size * ratio);
  const logical = Math.floor(physical / ratio);
  return { logical, physical };
}

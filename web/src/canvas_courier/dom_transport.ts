import type { CanvasCourierSurface } from "./types";
import { SharedCanvasEventAction, createSharedCanvasInputQueue } from "./ring_buffer";
import type {
  LogicalCanvasSize,
  SharedCanvasInputConfig,
  SharedCanvasInputQueue,
} from "./ring_buffer";

export class CanvasCourierTransport {
  private activeSurface: CanvasCourierSurface | undefined;
  private attachmentAbortController: AbortController | undefined;
  private logicalCanvasSize: LogicalCanvasSize | undefined;
  private readonly inputQueue: SharedCanvasInputQueue;
  private resizeObserver: ResizeObserver | undefined;

  constructor() {
    this.inputQueue = createSharedCanvasInputQueue();
  }

  get inputConfig(): SharedCanvasInputConfig {
    return this.inputQueue.config;
  }

  currentSize(): LogicalCanvasSize {
    if (!this.logicalCanvasSize) {
      throw new Error("Canvas Courier transport size is not initialized yet.");
    }

    return this.logicalCanvasSize;
  }

  measureSurface(surface: CanvasCourierSurface): LogicalCanvasSize {
    const bounds = surface.container.getBoundingClientRect();
    const fallbackWidth = surface.canvas.clientWidth || surface.canvas.width;
    const fallbackHeight = surface.canvas.clientHeight || surface.canvas.height;
    const width = bounds.width > 0 ? bounds.width : fallbackWidth;
    const height = bounds.height > 0 ? bounds.height : fallbackHeight;
    const scaleFactor = window.devicePixelRatio;

    return {
      width: this.snapToDevicePixel(width, scaleFactor),
      height: this.snapToDevicePixel(height, scaleFactor),
      scaleFactor,
    };
  }

  applyVisibleCanvasSize(canvas: HTMLCanvasElement, size: LogicalCanvasSize): void {
    canvas.style.width = `${size.width}px`;
    canvas.style.height = `${size.height}px`;
  }

  attachSurface(surface: CanvasCourierSurface): void {
    this.releaseSurfaceBindings();
    this.activeSurface = surface;

    const abortController = new AbortController();
    const listenerOptions = { signal: abortController.signal } as const;

    this.attachmentAbortController = abortController;
    this.syncSurfaceSize(surface);

    surface.canvas.addEventListener(
      "keydown",
      (event) => {
        this.inputQueue.writer.enqueueKeyboard(event);
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "keyup",
      (event) => {
        this.inputQueue.writer.enqueueKeyboard(event);
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "blur",
      () => {
        this.inputQueue.writer.enqueueBlur(performance.now());
      },
      listenerOptions,
    );

    document.addEventListener(
      "visibilitychange",
      () => {
        this.inputQueue.writer.enqueueVisibility(document.hidden);
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "pointermove",
      (event) => {
        this.inputQueue.writer.enqueuePointer(
          event,
          event.offsetX,
          event.offsetY,
          SharedCanvasEventAction.Move,
        );
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "pointerdown",
      (event) => {
        surface.canvas.focus({ preventScroll: true });
        this.inputQueue.writer.enqueuePointer(
          event,
          event.offsetX,
          event.offsetY,
          SharedCanvasEventAction.Down,
        );
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "pointerup",
      (event) => {
        this.inputQueue.writer.enqueuePointer(
          event,
          event.offsetX,
          event.offsetY,
          SharedCanvasEventAction.Up,
        );
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "pointerleave",
      (event) => {
        this.inputQueue.writer.enqueuePointerLeave(event.timeStamp);
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "pointercancel",
      (event) => {
        this.inputQueue.writer.enqueuePointerLeave(event.timeStamp);
      },
      listenerOptions,
    );

    surface.canvas.addEventListener(
      "wheel",
      (event) => {
        this.inputQueue.writer.enqueueWheel(event);
      },
      { signal: abortController.signal, passive: true },
    );

    this.resizeObserver = new ResizeObserver(() => {
      this.syncSurfaceSize(surface);
    });
    this.resizeObserver.observe(surface.container);

    this.inputQueue.writer.enqueueVisibility(document.hidden);
  }

  detachSurface(canvas: HTMLCanvasElement): void {
    if (this.activeSurface?.canvas !== canvas) {
      return;
    }

    this.enqueueDetachEvents();
    this.activeSurface = undefined;
    this.releaseSurfaceBindings();
  }

  dispose(): void {
    this.activeSurface = undefined;
    this.logicalCanvasSize = undefined;
    this.releaseSurfaceBindings();
  }

  private syncSurfaceSize(surface: CanvasCourierSurface): void {
    const nextSize = this.measureSurface(surface);
    this.logicalCanvasSize = nextSize;
    this.applyVisibleCanvasSize(surface.canvas, nextSize);
    this.inputQueue.writer.enqueueResize(nextSize);
  }

  private enqueueDetachEvents(): void {
    this.inputQueue.writer.enqueuePointerLeave();
    this.inputQueue.writer.enqueueBlur(performance.now());
  }

  private releaseSurfaceBindings(): void {
    this.attachmentAbortController?.abort();
    this.attachmentAbortController = undefined;
    this.resizeObserver?.disconnect();
    this.resizeObserver = undefined;
  }

  private snapToDevicePixel(size: number, ratio: number): number {
    return Math.floor(Math.floor(size * ratio) / ratio);
  }
}

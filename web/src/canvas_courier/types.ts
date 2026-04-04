export interface CanvasCourierSurface {
  canvas: HTMLCanvasElement;
  container: HTMLElement;
  offscreen: OffscreenCanvas;
}

// Transport implementations handle DOM I/O only. App-level controllers can wrap a transport
// and add async worker/session lifecycle before satisfying this interface.
export interface CanvasCourierController {
  attachSurface(surface: CanvasCourierSurface): Promise<void>;
  detachSurface(canvas: HTMLCanvasElement): void;
  scheduleDispose(): void;
}

export interface CanvasCourierStatus {
  attached: boolean;
  error: Error | null;
}

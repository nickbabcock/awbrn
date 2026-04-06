/**
 * Canvas Courier is an input event pipeline for offscreen canvases in web
 * workers.
 *
 * It handles plumbing all canvas-related input events (pointer, keyboard,
 * wheel, focus, resize, and visibility) from the UI thread to the worker.
 * Events are written atomically into a SharedArrayBuffer ring buffer, which
 * bypasses postMessage entirely to avoid serialization overhead and to keep
 * input on a dedicated channel, leaving the worker's message port free for
 * application use. The worker side drains the buffer via
 * `SharedCanvasInputReader`.
 *
 * The ring buffer holds up to 512 events, ensuring that bursts of input between
 * worker frames are not dropped.
 *
 * This library is analogous to winit in many aspects.
 *
 * `useCanvasCourierSurface` is a React hook that manages attaching, detaching,
 * and transferring the canvas offscreen in a React Strict Mode-compliant way.
 *
 * Note: SharedArrayBuffer requires cross-origin isolation (COOP/COEP headers).
 *
 * [0]: https://nolanlawson.com/2019/08/14/browsers-input-events-and-frame-throttling/
 */

export { CanvasCourierTransport } from "./dom_transport";
export { WebKeyCode } from "./key_codes.generated";
export {
  SharedCanvasInputReader,
  SharedCanvasEventType,
  SharedCanvasEventAction,
  SharedCanvasWheelDeltaMode,
  type LogicalCanvasSize,
  type SharedCanvasInputConfig,
  type SharedCanvasDecodedEvent,
} from "./ring_buffer";
export { useCanvasCourierSurface } from "./useCanvasCourierSurface";
export type { CanvasCourierController, CanvasCourierStatus, CanvasCourierSurface } from "./types";

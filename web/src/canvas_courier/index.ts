/**
 * ARing Buffer, it captures every click, keypress, and 1000Hz mouse move. This
 * prevents the "vanishing event" problem common in state-based architectures,
 * ensuring no user action is lost between frames
 *
 *
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

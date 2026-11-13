import type { TileRect } from "#/wasm/awbrn_wasm.js";

/** A scale this close to another is the same scale. */
export const SCALE_EPSILON = 0.01;

/**
 * The whole map a player opened, and the view they left to open it.
 *
 * Opening asks the engine to fit the board, and the engine answers with a
 * `BoardView` at the fit scale some frames later. Until it does, every view
 * the page holds is still the one the player left, so a zoom read off it is
 * not a zoom away from the whole map: `awaitingFit` holds the saved view until
 * the fit has arrived.
 */
export interface WholeMapState {
  /** The view the player left, while the whole map is showing. */
  left: TileRect | null;
  /** True from the open until the board first reaches the fit scale. */
  awaitingFit: boolean;
}

export const WHOLE_MAP_CLOSED: WholeMapState = { left: null, awaitingFit: false };

export function openWholeMap(left: TileRect): WholeMapState {
  return { left, awaitingFit: true };
}

/**
 * Follow the board's view: the fit arriving settles the open, and a zoom past
 * the fit afterwards (a wheel, a pinch, a tap below the touch floor) leaves
 * the whole map without a way back. A board that is gone takes its way back
 * with it. The state comes back unchanged when nothing moved, so a caller can
 * skip the update.
 */
export function followBoardView(
  state: WholeMapState,
  bv: { scale: number; fitScale: number } | null,
): WholeMapState {
  if (state.left === null) return state;
  if (bv === null) return WHOLE_MAP_CLOSED;
  const isAtFit = bv.scale <= bv.fitScale + SCALE_EPSILON;
  if (state.awaitingFit) {
    return isAtFit ? { left: state.left, awaitingFit: false } : state;
  }
  return isAtFit ? state : WHOLE_MAP_CLOSED;
}

/**
 * Press and release one key on the board's canvas, as a key the player struck.
 *
 * A key on the page that does what a board key does (Deselect is Escape)
 * sends that key, so the board answers it the one way it answers the
 * keyboard. The events go through the same listeners a real key does.
 */
export function pressBoardKey(canvas: HTMLCanvasElement, code: "Escape"): void {
  const init = { bubbles: true, cancelable: true, code, key: code };
  canvas.dispatchEvent(new KeyboardEvent("keydown", init));
  canvas.dispatchEvent(new KeyboardEvent("keyup", init));
}

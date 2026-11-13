/**
 * Press and release one key on the board's canvas, so a page key that does
 * what a board key does (Deselect is Escape) goes through the same listeners.
 */
export function pressBoardKey(canvas: HTMLCanvasElement, code: "Escape"): void {
  const init = { bubbles: true, cancelable: true, code, key: code };
  canvas.dispatchEvent(new KeyboardEvent("keydown", init));
  canvas.dispatchEvent(new KeyboardEvent("keyup", init));
}

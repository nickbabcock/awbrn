import * as stylex from "@stylexjs/stylex";

/**
 * Geometry the calculator's strip and table share.
 *
 * The figure columns are fixed rather than fluid because the point of the table
 * is that two engagements can be compared without reading either one: a column
 * that resized per row would make the comparison a reading task again.
 */
export const battleCalculatorLayout = stylex.defineConsts({
  // Wide enough for "TAKE 1ST", so the two damage lines hang their figures in
  // one column whichever of them is in front.
  figureLabelInlineSize: "calc(var(--spacing-12) + var(--spacing-2) + var(--spacing-0-5))",
  // The label plus "105 – 114%" at the HUD face, with the tracking it carries.
  // Only this column is labelled: the funds beside it are the same two facts in
  // the same two lines, so a second DEAL and TAKE would name them twice.
  damageColumnInlineSize:
    "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-3))",
  // "12,000 – 14,400" is the widest bracket a whole Mega Tank produces.
  fundsColumnInlineSize:
    "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-6) + var(--spacing-1))",
  // "+2,310 – +3,220" set one step above the HUD floor, on one line.
  netColumnInlineSize:
    "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-3) + var(--spacing-0-5))",
  // Four pickers on one line. It is a flex basis rather than a minimum: below
  // it the figures give up and wrap to their own line under the target, which
  // is what keeps the pickers from stacking three deep inside a board frame
  // whose width follows the window's height.
  targetColumnInlineSize:
    "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-6))",
  // The panel sits inside the board frame with the same inset the board menus
  // clear, and takes the height it is given rather than the height it wants.
  panelInset: "var(--spacing-2)",
  panelMaxInlineSize: "min(100%, 60rem)",
  // Sized to hold the art, the name and the caret. "Battleship" and "Missile
  // Silo" are the two names that still run past this and lose their last
  // letters to an ellipsis; both stay unmistakable from the sprite beside
  // them, and both are read whole in the grid the trigger opens.
  unitInlineSize:
    "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-2))",
  // A mark, two digits and a stepper. Health runs one to ten and never wider.
  healthInlineSize: "calc(var(--spacing-12) + var(--spacing-8))",
  terrainInlineSize:
    "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-12) + var(--spacing-6))",
  ammoInlineSize: "calc(var(--spacing-12) + var(--spacing-8))",
  // Two context columns while there is room for both, one below it.
  stripMedia: "@media (min-width: 720px)",
  /**
   * When the board frame is too small to hold the panel, and the panel becomes
   * a sheet instead.
   *
   * Height is in the rule because the board is 3:2 and its width is capped by
   * the window's height, so a short window makes a short board however wide the
   * screen is. A panel taller than the frame it sits in is a panel whose last
   * line is cut in half, and a bottom sheet is a better answer than a scroll
   * the player did not ask for.
   */
  sheetMedia: "(max-width: 1279px), (max-height: 899px)",
  sheetMaxBlockSize: "min(86svh, 46rem)",
  sheetBorderRadius: "var(--radius-container) var(--radius-container) 0 0",
});

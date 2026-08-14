import * as stylex from "@stylexjs/stylex";

/**
 * The geometry of a picker made of sprites.
 *
 * Every value here is measured off the art and off the longest name each
 * catalogue holds, rather than chosen: a cell too narrow for "Battleship"
 * breaks the word across two lines, and a bitmap face broken mid-word stops
 * being readable at all.
 */
export const spritePickerLayout = stylex.defineConsts({
  // "Battleship" and "Transport" set whole at the HUD face, plus the padding
  // either side of them.
  cellMinInlineSize: "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-2))",
  // A unit sprite at two, a name that may take two lines, and its cost. Each
  // cell is as tall as what it holds and no taller: the grid is a menu the
  // player is reading across, not a gallery.
  unitCellBlockSize: "calc(var(--spacing-12) + var(--spacing-10))",
  // A tile is a 16x32 cell, because half of it is what rises above the tile:
  // the peak, the roof, the tower. Drawn at two it is the tallest art here.
  terrainCellBlockSize: "calc(var(--spacing-12) + var(--spacing-12) + var(--spacing-6))",
  // A portrait is square, and every commander's name fits on one line.
  commanderCellBlockSize: "calc(var(--spacing-12) + var(--spacing-10))",
  // Four columns, sized to the cells rather than to the room a desk has: a
  // popover wider than its own content is a panel with a margin down the side,
  // and this one stands over a calculator that is itself compact.
  panelInlineSize: "min(calc(100vw - var(--spacing-6)), 30rem)",
  // Deep enough to show three rows whole, so a grid that scrolls says so by
  // cutting the fourth rather than by appearing complete.
  gridMaxBlockSize: "min(50vh, 20rem)",
});

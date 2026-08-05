import * as stylex from "@stylexjs/stylex";

/** Shared board-and-roster frame contract. Astryx layout regions are budgeted in px. */
export const rosterLayout = stylex.defineConsts({
  boardMaxInlineSize:
    "min(100%, max(var(--size-board-min), calc((100svh - var(--size-board-viewport-offset)) * 1.5)))",
  desktopMedia: "@media (min-width: 992px)",
  pairedRowsMedia: "@media (min-width: 640px) and (max-width: 991px)",
  // The board is 3:2 and must fit vertically, so its width is set by the window
  // height. A `1fr` board column is therefore wider than the board on a wide
  // screen, and the difference becomes dead space between the board and the
  // rail. The column takes the board's own height-derived width instead, held
  // below the width the rail and the gap leave free so a narrow desktop window
  // shrinks the board rather than pushing the rail off screen. Pair this with
  // `justifyContent: "center"` on the grid; the spare width belongs outside the
  // two columns, not between them.
  railColumns:
    "minmax(0, min(max(var(--size-board-min), calc((100svh - var(--size-board-viewport-offset)) * 1.5)), calc(100% - var(--size-roster-rail) - var(--spacing-4)))) var(--size-roster-rail)",
});

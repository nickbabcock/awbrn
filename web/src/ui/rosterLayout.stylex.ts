import * as stylex from "@stylexjs/stylex";

/** Shared board-and-roster frame contract. Astryx layout regions are budgeted in px. */
export const rosterLayout = stylex.defineConsts({
  boardMaxInlineSize:
    "min(100%, max(var(--size-board-min), calc((100svh - var(--size-board-viewport-offset)) * 1.5)))",
  desktopMedia: "@media (min-width: 992px)",
  pairedRowsMedia: "@media (min-width: 640px) and (max-width: 991px)",
  railColumns: "minmax(0, 1fr) var(--size-roster-rail)",
});

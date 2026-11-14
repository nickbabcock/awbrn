import * as stylex from "@stylexjs/stylex";

/** Geometry shared by the board menu variants. */
export const boardMenuLayout = stylex.defineConsts({
  actionInlineSize: "148px",
  // A menu of engagements is wider than a menu of words, but only by what a
  // sprite and a percentage need. The header's own unit sets the floor.
  actionForecastInlineSize: "192px",
  actionRowSpaciousMinBlockSize: "48px",
  buildInlineSize: "232px",
  buildMaxInlineSize: "calc(100% - var(--spacing-4))",
  sheetBorderRadius: "var(--radius-container) var(--radius-container) 0 0",
});

/** Marks a menu row, so its caret can follow the row's focus. */
export const menuRowMarker = stylex.defineMarker();

import * as stylex from "@stylexjs/stylex";

/** Geometry shared by the board menu variants. */
export const boardMenuLayout = stylex.defineConsts({
  actionInlineSize: "148px",
  // A menu of engagements is wider than a menu of words, but only by what a
  // sprite and a percentage need. The header's own unit sets the floor.
  actionForecastInlineSize: "192px",
  // Wide enough for the longest label, so DEAL and TAKE hang their figures in
  // one column whichever of them is in front.
  forecastLabelInlineSize: "58px",
  actionRowMinBlockSize: "22px",
  actionRowSpaciousMinBlockSize: "48px",
  buildFundsLineMinBlockSize: "20px",
  buildInlineSize: "232px",
  buildMaxInlineSize: "calc(100% - var(--spacing-4))",
  buildRowMinBlockSize: "30px",
  buildRowSpaciousMinBlockSize: "56px",
  sheetBorderRadius: "var(--radius-container) var(--radius-container) 0 0",
});

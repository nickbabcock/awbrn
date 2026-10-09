import * as stylex from "@stylexjs/stylex";

/**
 * The page frame every screen shares.
 *
 * One measure, one gutter, and one breakpoint, so a page reads as part of the
 * same product as the page before it. Structural widths are the one place raw
 * px belongs; everything inside a page uses the spacing scale.
 */
export const pageLayout = stylex.defineConsts({
  /** The measure of an ordinary page: wide enough for a briefing beside a queue. */
  standardWidth: "1240px",
  /** The measure of a page that is mostly prose or one form. */
  narrowWidth: "760px",
  /** Where a page takes its desktop arrangement. */
  desktopMedia: "@media (min-width: 960px)",
  /** Where a page is a phone, and its gutter closes in. */
  phoneMedia: "@media (max-width: 599px)",
});

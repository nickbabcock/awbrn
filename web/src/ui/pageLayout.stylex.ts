import * as stylex from "@stylexjs/stylex";

/**
 * The page frame every screen shares. Structural widths are the one place raw
 * px belongs; everything inside a page uses the spacing scale.
 */
export const pageLayout = stylex.defineConsts({
  standardWidth: "1240px",
  /** Content width; the gutters sit outside it. */
  narrowWidth: "880px",
  railWidth: "392px",
  gutterDesktop: "32px",
  gutterPhone: "16px",
  /**
   * A notched phone held sideways is desktop-wide, with its camera housing in
   * one side inset. A gutter is never narrower than the inset on its side.
   */
  safeInsetLeft: "env(safe-area-inset-left, 0px)",
  safeInsetRight: "env(safe-area-inset-right, 0px)",
  topBarHeight: "56px",
  topBarHeightPhone: "52px",
  tabBarHeight: "64px",
  /** Where a page sets its rail beside its main column. */
  desktopMedia: "@media (min-width: 1181px)",
  belowRailMedia: "@media (max-width: 1180px)",
  tabletMedia: "@media (min-width: 721px) and (max-width: 1180px)",
  phoneMedia: "@media (max-width: 720px)",
  coarseMedia: "@media (pointer: coarse)",
  /**
   * Hover tones live only under this, so a tap never leaves one behind; pair
   * each with the same tone on `:active` for touch.
   */
  hoverMedia: "@media (hover: hover)",
  reducedMotionMedia: "@media (prefers-reduced-motion: reduce)",
});

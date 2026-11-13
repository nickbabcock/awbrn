import * as stylex from "@stylexjs/stylex";

/**
 * The page frame every screen shares.
 *
 * One measure, one gutter, and the breakpoints DESIGN.md names: 720px and
 * below is a phone (16px gutters, the tab bar), and the rail beside a main
 * column drops below 1180px. Structural widths are the one place raw px
 * belongs; everything inside a page uses the spacing scale (4px steps).
 */
export const pageLayout = stylex.defineConsts({
  /** A paper page: at most 1240px, gutters included (1176px of content). */
  standardWidth: "1240px",
  /** A wide paper page (a board beside its panel), gutters included. */
  wideWidth: "1440px",
  /**
   * A single-list page (My games) centers 880px of content, gutters outside,
   * instead of leaving a band beside it.
   */
  narrowWidth: "880px",
  /** The rail beside a main column on a paper page. */
  railWidth: "392px",
  /** Page gutters. */
  gutterDesktop: "32px",
  gutterPhone: "16px",
  /**
   * A notched phone held sideways is desktop-wide, with its camera housing in
   * one side inset. A gutter is never narrower than the inset on its side:
   * `max(gutter, safeInsetLeft)` on the start (the site reads left to right).
   */
  safeInsetLeft: "env(safe-area-inset-left, 0px)",
  safeInsetRight: "env(safe-area-inset-right, 0px)",
  /** The navy top bar, and the phone tab bar under the content. */
  topBarHeight: "56px",
  topBarHeightPhone: "52px",
  tabBarHeight: "64px",
  /** Where a page sets its rail beside its main column. */
  desktopMedia: "@media (min-width: 1181px)",
  /** Where the rail has dropped below the main column (tablet and phone). */
  belowRailMedia: "@media (max-width: 1180px)",
  /** Between the phone and the rail: a tablet, a small laptop, a phone held sideways. */
  tabletMedia: "@media (min-width: 721px) and (max-width: 1180px)",
  /** Where a page is a phone: 16px gutters, the tab bar, 44px targets. */
  phoneMedia: "@media (max-width: 720px)",
  /** Anything wider than a phone. */
  notPhoneMedia: "@media (min-width: 721px)",
  /** A coarse primary pointer: every target at least 44px. */
  coarseMedia: "@media (pointer: coarse)",
  /**
   * A pointer that can hover. Hover tones live only under it, so a tap never
   * leaves one behind; pair each with the same tone on `:active` for touch.
   */
  hoverMedia: "@media (hover: hover)",
  /** Motion is a step or nothing; under this query it is nothing. */
  reducedMotionMedia: "@media (prefers-reduced-motion: reduce)",
});

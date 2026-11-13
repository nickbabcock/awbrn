import * as stylex from "@stylexjs/stylex";

/**
 * What the shell shows around a screen, as variables the shell root sets.
 *
 * A screen declares its kind by rendering `<ShellMode mode="pushed" />` (or
 * "task"). The root reads that marker with `:has()`, so the right chrome is in
 * the first server-rendered paint with no state and no flash. Only the shell
 * reads these; screens use `ShellMode`.
 */
export const shellMarker = stylex.defineMarker();

export const shellVars = stylex.defineVars({
  /** The phone tab bar: shown on section pages, hidden on pushed and task screens. */
  tabBarDisplay: "grid",
  /** The top bar on a phone: hidden on a pushed screen (the match, a replay). */
  topBarPhoneDisplay: "flex",
  /** The independence footer: hidden on a pushed screen at every width. */
  footerDisplay: "flex",
  /** The top bar's New match key (a `contents` slot): hidden on a pushed screen. */
  newMatchDisplay: "contents",
  /**
   * Room the page leaves under its content for the fixed phone tab bar. The
   * home indicator's inset under it is AppShell's own padding on main.
   */
  tabBarSpace: "64px",
});

import * as stylex from "@stylexjs/stylex";

/**
 * What the shell shows around a screen, as variables the shell root sets from
 * the `ShellMode` marker with `:has()`, so the right chrome is in the first
 * server-rendered paint. Screens use `ShellMode`, not these.
 */
export const shellMarker = stylex.defineMarker();

export const shellVars = stylex.defineVars({
  tabBarDisplay: "grid",
  topBarPhoneDisplay: "flex",
  footerDisplay: "flex",
  newMatchDisplay: "contents",
  /**
   * Room the page leaves under its content for the fixed phone tab bar. The
   * home indicator's inset under it is AppShell's own padding on main.
   */
  tabBarSpace: "64px",
});

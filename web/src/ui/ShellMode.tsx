import { Stack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { shellMarker } from "#/ui/shell.stylex.ts";

/**
 * How the shell frames a screen. Render one inside a screen that is not an
 * ordinary paper page; the shell reads it with `:has()` from the first paint.
 *
 * - `pushed`: the match and the replay viewer. On a phone there is no top bar
 *   and no tab bar (the screen draws its own turn line with a `BackKey`); at
 *   every width there is no footer and no New match key in the top bar. The
 *   screen owns `100dvh` on a phone and `100dvh - 56px` on desktop.
 * - `task`: New match, match setup and a ranked pairing. The tab bar steps
 *   aside on a phone so the screen can dock its task dock; the top bar and the
 *   footer stay.
 *
 *   <ShellMode mode="pushed" />
 *
 * Paper pages render nothing: `section` is the default.
 */
export function ShellMode({ mode }: { mode: "pushed" | "task" }) {
  // A marker is not a style, so it goes on as the class `stylex.props` gives
  // it rather than through `xstyle`.
  return <Stack as="span" data-shell-mode={mode} hidden {...stylex.props(shellMarker)} />;
}

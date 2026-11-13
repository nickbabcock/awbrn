import { Stack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { shellMarker } from "#/ui/shell.stylex.ts";

/**
 * How the shell frames a screen that is not an ordinary paper page.
 *
 * - `pushed` (the match, the replay viewer): no footer and no New match key;
 *   on a phone no top bar and no tab bar either, so the screen draws its own
 *   `BackKey`.
 * - `task` (match setup, a ranked pairing): the tab bar steps aside on a phone
 *   so the screen can dock its task dock.
 */
export function ShellMode({ mode }: { mode: "pushed" | "task" }) {
  // A marker is not a style, so it goes on as the class `stylex.props` gives
  // it rather than through `xstyle`.
  return <Stack as="span" data-shell-mode={mode} hidden {...stylex.props(shellMarker)} />;
}

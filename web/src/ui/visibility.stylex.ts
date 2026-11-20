import * as stylex from "@stylexjs/stylex";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

export const visibility = stylex.create({
  /** Hidden on a phone: the map editor is desktop only. */
  desktopOnly: {
    display: {
      default: null,
      [pageLayout.phoneMedia]: "none",
    },
  },
});

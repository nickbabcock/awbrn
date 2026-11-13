import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/**
 * Heading looks that differ from the heading's level: a sheet head is an h2
 * in the outline but reads in the title style (17px, full width), the way
 * `<h2 class="h3">` does in the design.
 */
export const headingLooks = stylex.create({
  title: {
    fontSize: awbrnVars.text17,
    fontWeight: 700,
    lineHeight: 1.25,
    fontStretch: "100%",
    letterSpacing: "normal",
    wordSpacing: "normal",
  },
});

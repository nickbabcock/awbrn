import { focusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "./pageLayout.stylex.ts";
import { rowLinkMarker } from "./rowLink.stylex.ts";

const styles = stylex.create({
  stretch: {
    outlineStyle: "none",
    "::after": {
      content: '""',
      position: "absolute",
      inset: 0,
    },
  },
  row: {
    position: "relative",
    outlineColor: awbrnVars.blue,
    outlineOffset: `calc(-1 * ${focusVars["--focus-outline-offset"]})`,
    outlineStyle: {
      default: "none",
      [stylex.when.descendant(":focus-visible", rowLinkMarker)]: "solid",
    },
    outlineWidth: focusVars["--focus-outline-width"],
    backgroundColor: {
      default: null,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
  },
});

/**
 * A row that is one link: `stretch` on the link spreads it over the row, and
 * `row` on the positioned row wears that link's focus ring, while the other
 * links inside it (seat names) keep their own.
 */
export const rowLinkStyles = {
  stretch: [rowLinkMarker, styles.stretch],
  row: styles.row,
} as const;

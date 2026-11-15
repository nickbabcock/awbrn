import { focusVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { rowLinkStyles } from "#/ui/rowLink.ts";

/**
 * A player's name as the way to their profile. It reads as the name until
 * hovered, so a list of names stays a list rather than a column of links.
 *
 * `reach` sets what the link covers: a 44px target around the name (which
 * also lifts a seat above a row's own stretched link), or the whole
 * positioned row, which then wears the focus ring (`rowLinkStyles.row`). A
 * long name ellipsizes inside the link, never on it, or the target is clipped.
 */
export function PlayerLink({
  children,
  onClick,
  reach = "target",
  userId,
  xstyle,
}: {
  children: ReactNode;
  onClick?: () => void;
  reach?: "target" | "row";
  userId: string;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <Link
      onClick={onClick}
      params={{ playerId: userId }}
      to="/players/$playerId"
      {...stylex.props(
        styles.link,
        reach === "row" ? rowLinkStyles.stretch : [styles.ring, styles.target],
        xstyle,
      )}
    >
      {children}
    </Link>
  );
}

const styles = stylex.create({
  link: {
    color: {
      default: "inherit",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.blue },
    },
    fontWeight: "inherit",
    textDecorationLine: {
      default: "none",
      ":hover": { default: null, [pageLayout.hoverMedia]: "underline" },
    },
    textDecorationColor: awbrnVars.blue,
    textDecorationThickness: "1px",
    textUnderlineOffset: "3px",
  },
  ring: {
    borderRadius: radiusVars["--radius-inner"],
    outlineColor: awbrnVars.blue,
    outlineOffset: focusVars["--focus-outline-offset"],
    outlineStyle: { default: "none", ":focus-visible": "solid" },
    outlineWidth: focusVars["--focus-outline-width"],
  },
  target: {
    position: "relative",
    zIndex: 1,
    display: "block",
    inlineSize: "fit-content",
    maxInlineSize: "100%",
    minInlineSize: 0,
    // Centered on the name, a short handle's target is still 44px wide.
    "::before": {
      content: '""',
      position: "absolute",
      insetInlineStart: "50%",
      insetBlockStart: "50%",
      inlineSize: `max(calc(100% + 2 * ${spacingVars["--spacing-1-5"]}), 44px)`,
      blockSize: `max(calc(100% + 2 * ${spacingVars["--spacing-3"]}), 44px)`,
      transform: "translate(-50%, -50%)",
    },
  },
});

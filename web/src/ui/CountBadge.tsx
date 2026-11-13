import { Text } from "@astryxdesign/core/Text";
import { colorVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/**
 * A count, and nothing else.
 *
 * - `tone="red"` (default): the one nav count badge, on My games (top bar and
 *   tab bar). The shell draws it; screens should not add another red count.
 * - `tone="ink"` with `size="heading"`: the count after a page or sheet head,
 *   `Your move 3`, sized from the heading's own font size (`em`).
 *
 * The number reads as a bare number aloud, so pass `label` with what it counts
 * ("3 matches") whenever the surrounding words do not already say it.
 *
 *   <Heading level={1}>Your move <CountBadge count={3} label="3 matches" size="heading" tone="ink" /></Heading>
 */
export function CountBadge({
  count,
  label,
  size = "nav",
  tone = "red",
  xstyle,
}: {
  count: number;
  label?: string;
  size?: "nav" | "heading";
  tone?: "red" | "ink";
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <Text
      aria-label={label}
      role={label ? "img" : undefined}
      type="inherit"
      xstyle={[
        styles.base,
        size === "nav" ? styles.nav : styles.heading,
        tone === "red" ? styles.red : styles.ink,
        xstyle,
      ]}
    >
      {count}
    </Text>
  );
}

const styles = stylex.create({
  base: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    fontVariantNumeric: "tabular-nums",
    lineHeight: 1,
    color: colorVars["--color-on-accent"],
  },
  nav: {
    minInlineSize: "20px",
    blockSize: "20px",
    paddingInline: spacingVars["--spacing-1-5"],
    borderRadius: radiusVars["--radius-full"],
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.textNav,
    fontWeight: 700,
  },
  heading: {
    minInlineSize: "1.15em",
    blockSize: "1.15em",
    marginInlineStart: "0.12em",
    paddingInline: "0.18em",
    borderRadius: "0.2em",
    verticalAlign: "0.08em",
    fontFamily: awbrnVars.fontFigure,
    fontSize: "0.5em",
    fontWeight: 600,
    fontStretch: "87.5%",
    letterSpacing: "-0.02em",
  },
  red: {
    backgroundColor: awbrnVars.red,
  },
  ink: {
    backgroundColor: awbrnVars.ink900,
  },
});

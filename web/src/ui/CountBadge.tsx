import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { colorVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/**
 * The red tone is the shell's one nav count, on My games; screens should not
 * add another. The number reads as a bare number aloud, so pass `label` with
 * what it counts whenever the surrounding words do not already say it.
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

/** The nav count of matches waiting on the player, with its words for assistive tech. */
export function AwaitingCount({ count, xstyle }: { count: number; xstyle?: stylex.StyleXStyles }) {
  if (count <= 0) return null;
  return (
    <>
      <CountBadge count={count} xstyle={xstyle} />
      <VisuallyHidden>
        {count === 1 ? "1 match waits on you" : `${count} matches wait on you`}
      </VisuallyHidden>
    </>
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

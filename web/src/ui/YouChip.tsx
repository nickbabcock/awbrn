import { HStack } from "@astryxdesign/core/Stack";
import { radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/** YOU, on Ink 900: the viewer's own row in the standings, and their own profile. */
export function YouChip({ xstyle }: { xstyle?: stylex.StyleXStyles }) {
  return (
    <HStack as="span" xstyle={[styles.you, xstyle]}>
      YOU
    </HStack>
  );
}

const styles = stylex.create({
  you: {
    display: "inline-flex",
    alignItems: "center",
    flexShrink: 0,
    blockSize: "18px",
    paddingInline: spacingVars["--spacing-1-5"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.textMicro,
    fontWeight: 750,
    letterSpacing: "0.14em",
  },
});

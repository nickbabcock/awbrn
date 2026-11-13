import { HStack } from "@astryxdesign/core/Stack";
import { borderVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * The task dock (phone only): the choice so far and the one red key stay under
 * the thumb while a long form scrolls. A task screen renders
 * `<ShellMode mode="task" />` so the tab bar steps aside for it, and puts the
 * dock last in its page so it sticks to the bottom of the screen.
 *
 * `isOwed` runs the airmail band along its top (a pairing to confirm).
 * `keys` are one key, or two side by side (Decline | Ready).
 */
export function TaskDock({
  isOwed = false,
  keys,
  label,
  line,
}: {
  isOwed?: boolean;
  keys: ReactNode;
  label: string;
  line: ReactNode;
}) {
  return (
    <section aria-label={label} {...stylex.props(styles.dock)}>
      {isOwed ? <AirmailBand thickness={5} xstyle={styles.band} /> : null}
      <HStack xstyle={styles.line}>{line}</HStack>
      <HStack xstyle={styles.keys}>{keys}</HStack>
    </section>
  );
}

/** The paper page's bottom padding on a phone (`Page` in `#/ui/Page.tsx`). */
const PAGE_FOOT_PHONE = "40px";

const styles = stylex.create({
  dock: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "grid",
    },
    position: "sticky",
    insetBlockEnd: 0,
    zIndex: 20,
    gap: awbrnVars.spacing2_5,
    marginInline: `calc(${pageLayout.gutterPhone} * -1)`,
    // At the end of the page the dock meets the footer: it takes the paper
    // page's 40px phone foot (Page's frame) rather than leaving a band of
    // empty paper between them.
    marginBlockEnd: `calc(${PAGE_FOOT_PHONE} * -1)`,
    paddingBlockStart: spacingVars["--spacing-3"],
    paddingInline: pageLayout.gutterPhone,
    paddingBlockEnd: `calc(${spacingVars["--spacing-3"]} + env(safe-area-inset-bottom, 0px))`,
    backgroundColor: awbrnVars.sheet,
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    boxShadow: awbrnVars.shadowDock,
  },
  band: {
    marginBlockStart: `calc(${spacingVars["--spacing-3"]} * -1)`,
    marginInline: `calc(${pageLayout.gutterPhone} * -1)`,
    inlineSize: "auto",
  },
  line: {
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    minInlineSize: 0,
  },
  keys: {
    display: "flex",
    gap: spacingVars["--spacing-2"],
  },
});

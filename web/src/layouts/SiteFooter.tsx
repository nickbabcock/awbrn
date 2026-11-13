import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { RouterTextLink } from "#/ui/astryx-links.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { shellVars } from "#/ui/shell.stylex.ts";

/**
 * The foot of every paper page, at both widths: AWBRN is independent, said
 * once, so no screen has to make room for it and no screen can be read as
 * implying otherwise. A pushed screen (the match, a replay) has no foot.
 */
export function SiteFooter() {
  return (
    <footer {...stylex.props(styles.footer)}>
      <Text color="secondary" type="inherit">
        AWBRN is independent and not affiliated with AWBW, Nintendo or Intelligent Systems.
      </Text>
      <RouterTextLink hasUnderline to="/about">
        About
      </RouterTextLink>
    </footer>
  );
}

const styles = stylex.create({
  footer: {
    display: shellVars.footerDisplay,
    flexDirection: {
      default: "row",
      [pageLayout.phoneMedia]: "column",
    },
    alignItems: {
      default: "center",
      [pageLayout.phoneMedia]: "flex-start",
    },
    justifyContent: "space-between",
    flexWrap: "wrap",
    gap: {
      default: "24px",
      [pageLayout.phoneMedia]: "8px",
    },
    paddingBlock: {
      default: "20px 28px",
      [pageLayout.phoneMedia]: "16px 20px",
    },
    paddingInlineStart: {
      default: `max(${pageLayout.gutterDesktop}, ${pageLayout.safeInsetLeft})`,
      [pageLayout.phoneMedia]: `max(${pageLayout.gutterPhone}, ${pageLayout.safeInsetLeft})`,
    },
    paddingInlineEnd: {
      default: `max(${pageLayout.gutterDesktop}, ${pageLayout.safeInsetRight})`,
      [pageLayout.phoneMedia]: `max(${pageLayout.gutterPhone}, ${pageLayout.safeInsetRight})`,
    },
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
    fontSize: {
      default: awbrnVars.text13,
      [pageLayout.phoneMedia]: awbrnVars.text12_5,
    },
  },
});

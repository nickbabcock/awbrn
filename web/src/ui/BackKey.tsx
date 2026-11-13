import { Icon } from "@astryxdesign/core/Icon";
import { colorVars, radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { createLink } from "@tanstack/react-router";
import { forwardRef } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { IconBack } from "#/ui/icons.tsx";

interface BackHostProps {
  label: string;
  href?: string;
  className?: string;
  style?: React.CSSProperties;
}

const BackLinkHost = forwardRef<HTMLAnchorElement, BackHostProps>(function BackLinkHost(
  { className: _className, label, style: _style, ...props },
  ref,
) {
  return (
    <a ref={ref} {...props} {...stylex.props(styles.link)}>
      <Icon icon={IconBack} size="md" />
      {label}
    </a>
  );
});

/** The way back on a paper page, above the title, named for where it goes. */
export const BackLink = createLink(BackLinkHost);

const BackKeyHost = forwardRef<HTMLAnchorElement, BackHostProps>(function BackKeyHost(
  { className: _className, label, style: _style, ...props },
  ref,
) {
  return (
    <a aria-label={label} ref={ref} {...props} {...stylex.props(styles.key)}>
      <Icon icon={IconBack} size="lg" />
    </a>
  );
});

/** The icon-only back key of a pushed screen on a phone, on ink. */
export const BackKey = createLink(BackKeyHost);

const styles = stylex.create({
  link: {
    display: "inline-flex",
    alignItems: "center",
    alignSelf: "flex-start",
    gap: "4px",
    minBlockSize: "44px",
    marginBlock: "-6px 8px",
    marginInlineStart: "-8px",
    paddingInline: "4px 12px",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paperWell },
      ":active": awbrnVars.paperWell,
    },
    color: {
      default: colorVars["--color-text-secondary"],
      ":hover": { default: null, [pageLayout.hoverMedia]: colorVars["--color-text-primary"] },
      ":active": colorVars["--color-text-primary"],
    },
    fontWeight: 600,
    textDecoration: "none",
  },
  key: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    inlineSize: "44px",
    blockSize: "44px",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.ink850 },
      ":active": awbrnVars.ink850,
    },
    color: awbrnVars.textOnInkStrong,
  },
});

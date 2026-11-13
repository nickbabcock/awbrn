import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Icon } from "@astryxdesign/core/Icon";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { createLink } from "@tanstack/react-router";
import { forwardRef, type ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconNext } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

interface ListRowHostProps {
  /** A thumbnail (56px; 80px in a match row), a stamp or a sprite. */
  media?: ReactNode;
  /** The row's name, bold. Map names are never mono. */
  title: ReactNode;
  /** One postage line under the title: `2P · 22×18 · cookielamb3`, terms, `1 / 2`. */
  meta?: ReactNode;
  /** What sits at the end instead of the chevron (a time left, a medal). */
  end?: ReactNode;
  /** The row is the link; the chevron says so. Off when `end` already does. */
  hasChevron?: boolean;
  href?: string;
  className?: string;
  style?: React.CSSProperties;
}

const ListRowHost = forwardRef<HTMLAnchorElement, ListRowHostProps>(function ListRowHost(
  { end, hasChevron = true, media, meta, title, className: _className, style: _style, ...props },
  ref,
) {
  return (
    <a ref={ref} {...props} {...stylex.props(styles.row)}>
      {media}
      <Stack as="span" xstyle={styles.main}>
        <b {...stylex.props(styles.title)}>{title}</b>
        {meta ? (
          <Text type="inherit" xstyle={styles.meta}>
            {meta}
          </Text>
        ) : null}
      </Stack>
      {end}
      {hasChevron ? <Icon icon={IconNext} size="md" xstyle={styles.chevron} /> : null}
    </a>
  );
});

/**
 * A list row that is one link: media, a bold title over one postage line, and
 * a chevron. 64px minimum, 12px by 20px padding (16px on a phone), a paper
 * hover. No dead middle: the title takes the slack.
 *
 * Put it in a `SheetRow` inside `SheetRows`. It takes TanStack Router's link
 * props (`to`, `params`, `search`).
 *
 *   <RouterListRow media={<Thumbnail ... />} meta="Live 5m +2m · 1 / 2"
 *     params={{ matchId }} title="Quick one on Normandie" to="/matches/$matchId" />
 */
export const RouterListRow = createLink(ListRowHost);

const styles = stylex.create({
  row: {
    display: "flex",
    alignItems: "center",
    gap: "12px",
    minBlockSize: "64px",
    paddingBlock: "12px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
    color: colorVars["--color-text-primary"],
    textDecoration: "none",
    transitionProperty: "background-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
  },
  main: {
    display: "flex",
    flexDirection: "column",
    gap: "2px",
    flexGrow: 1,
    minInlineSize: 0,
  },
  // Long names hold one line and end in an ellipsis; the whole name stays in
  // the text, so it is still the row's accessible name.
  title: {
    overflow: "hidden",
    fontWeight: 700,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  // The postage line may carry clocks and figures, which are never cut.
  meta: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
  },
  chevron: {
    color: awbrnVars.textTertiary,
    flexShrink: 0,
  },
});

import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Link, useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { sectionForPath, type SiteSection } from "#/ui/site_sections.ts";

/** One paper page: a centered measure with gutters that respect the safe-area insets. */
export function Page({
  children,
  width = "standard",
}: {
  children: ReactNode;
  width?: "standard" | "narrow";
}) {
  return (
    <VStack gap={6} xstyle={[styles.frame, width === "standard" ? styles.standard : styles.narrow]}>
      {children}
    </VStack>
  );
}

/** The page title, its commands, and the other pages of its section as tabs. */
export function PageHeader({ actions, title }: { actions?: ReactNode; title: ReactNode }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const isSignedIn = useAppSession() !== null;
  const section = sectionForPath(pathname);

  return (
    <VStack gap={0} as="header">
      <HStack align="end" gap={6} justify="between" wrap="wrap" xstyle={styles.head}>
        <VStack gap={2} xstyle={styles.titleBlock}>
          {/* Focus lands here after a route change (`#/ui/arrivalFocus.ts`). */}
          <Heading level={1} tabIndex={-1} xstyle={styles.title}>
            {title}
          </Heading>
        </VStack>
        {actions ? (
          <HStack align="center" gap={2} wrap="wrap">
            {actions}
          </HStack>
        ) : null}
      </HStack>
      {section ? (
        <SectionTabs isSignedIn={isSignedIn} pathname={pathname} section={section} />
      ) : null}
    </VStack>
  );
}

function SectionTabs({
  isSignedIn,
  pathname,
  section,
}: {
  isSignedIn: boolean;
  pathname: string;
  section: SiteSection;
}) {
  const pages = section.pages.filter((page) => isSignedIn || !page.requiresSession);
  // A section with one page open to this viewer has nowhere to move sideways
  // to, so it shows no tabs at all rather than a single selected one.
  if (pages.length < 2) return null;
  const selected = pages.find((page) => page.matches(pathname))?.to ?? "";

  return (
    <nav aria-label={section.label} {...stylex.props(styles.tabs)}>
      {pages.map((page) => (
        <Link
          activeOptions={{ exact: true }}
          aria-current={page.to === selected ? "page" : undefined}
          key={page.to}
          to={page.to}
          {...stylex.props(styles.tab, page.to === selected && styles.tabSelected)}
        >
          {page.label}
        </Link>
      ))}
    </nav>
  );
}

const styles = stylex.create({
  frame: {
    boxSizing: "border-box",
    inlineSize: "100%",
    marginInline: "auto",
    paddingInlineStart: {
      default: `max(${pageLayout.gutterDesktop}, ${pageLayout.safeInsetLeft})`,
      [pageLayout.phoneMedia]: `max(${pageLayout.gutterPhone}, ${pageLayout.safeInsetLeft})`,
    },
    paddingInlineEnd: {
      default: `max(${pageLayout.gutterDesktop}, ${pageLayout.safeInsetRight})`,
      [pageLayout.phoneMedia]: `max(${pageLayout.gutterPhone}, ${pageLayout.safeInsetRight})`,
    },
    paddingBlockStart: {
      default: "36px",
      [pageLayout.phoneMedia]: "20px",
    },
    paddingBlockEnd: {
      default: "72px",
      [pageLayout.phoneMedia]: "40px",
    },
  },
  // The standard measure holds its gutters; the narrow one sets them outside.
  standard: {
    maxInlineSize: pageLayout.standardWidth,
  },
  narrow: {
    maxInlineSize: `calc(${pageLayout.narrowWidth} + 2 * ${pageLayout.gutterDesktop})`,
  },
  head: {
    marginBlockEnd: "16px",
  },
  // As wide as its words, so the focus ring on arrival hugs the title.
  title: {
    alignSelf: "flex-start",
    maxInlineSize: "100%",
  },
  // On a phone the title takes its own width, so a short title and its key
  // share one row and a long one still wraps the key under it.
  titleBlock: {
    minInlineSize: 0,
    flexBasis: {
      default: "32rem",
      [pageLayout.phoneMedia]: "auto",
    },
    flexGrow: 1,
  },
  // On a phone the strip bleeds to the screen edges and scrolls sideways
  // rather than wrapping.
  tabs: {
    display: "flex",
    gap: "4px",
    marginInline: {
      default: 0,
      [pageLayout.phoneMedia]: `calc(${pageLayout.gutterPhone} * -1)`,
    },
    paddingInline: {
      default: 0,
      [pageLayout.phoneMedia]: `calc(${pageLayout.gutterPhone} - 6px)`,
    },
    overflowX: "auto",
    scrollbarWidth: "none",
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
  },
  tab: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    minBlockSize: "44px",
    paddingBlock: "10px 12px",
    paddingInline: "14px",
    color: {
      default: colorVars["--color-text-secondary"],
      ":hover": { default: null, [pageLayout.hoverMedia]: colorVars["--color-text-primary"] },
      ":active": colorVars["--color-text-primary"],
    },
    textDecoration: "none",
    fontWeight: 600,
    whiteSpace: "nowrap",
  },
  tabSelected: {
    color: colorVars["--color-text-primary"],
    "::after": {
      content: '""',
      position: "absolute",
      insetInline: "10px",
      insetBlockEnd: "-1px",
      blockSize: awbrnVars.currentMark,
      backgroundColor: awbrnVars.ink900,
      borderStartStartRadius: awbrnVars.currentMark,
      borderStartEndRadius: awbrnVars.currentMark,
    },
  },
});

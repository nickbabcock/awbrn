import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Link, useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { sectionForPath, type SiteSection } from "#/ui/site_sections.ts";

type PageWidth = "standard" | "wide" | "narrow" | "full";

/**
 * The frame of one paper page: a centered measure on the paper ground with
 * one gutter (32px, 16px on a phone).
 *
 * - `standard`: max 1240px, gutters included. A main column plus a 360 to 420px rail
 *   (`pageLayout.railWidth`) that drops below the main column under 1180px
 *   (`pageLayout.belowRailMedia`).
 * - `narrow`: a single-list page (My games) centers 880px of content (gutters
 *   outside it) instead of leaving a band beside it.
 * - `wide`: max 1440px, gutters included.
 * - `full`: no measure and no gutter (a screen that lays out its own frame).
 *
 *   <Page><PageHeader title="Play" />...</Page>
 */
export function Page({ children, width = "standard" }: { children: ReactNode; width?: PageWidth }) {
  return (
    <VStack
      gap={6}
      xstyle={[
        styles.frame,
        width === "standard" && styles.standard,
        width === "wide" && styles.wide,
        width === "narrow" && styles.narrow,
        width === "full" && styles.full,
      ]}
    >
      {children}
    </VStack>
  );
}

/**
 * The head of a paper page: the title in the display cut, a figure only when
 * it carries live state (`Your move 3`), the page's commands, and the other
 * pages of its section as underline tabs.
 *
 * **Delete By Default**: a page head is the title. No subtitle, no eyebrow, no
 * helper sentence. `description` is kept only so older screens compile while
 * they are restyled; do not pass it in new work.
 *
 *   <PageHeader title="Play" />
 *   <PageHeader back={<BackLink to="/ranked" label="Ranked" />} title="1vs1 Normandie" />
 */
export function PageHeader({
  actions,
  back,
  description,
  figure,
  title,
}: {
  actions?: ReactNode;
  /** A `BackLink` above the title, for a page reached from a list. */
  back?: ReactNode;
  /** @deprecated Delete By Default: a page head is its title. */
  description?: ReactNode;
  /** A live count after the title, usually a `CountBadge` with `size="heading"`. */
  figure?: ReactNode;
  title: ReactNode;
}) {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const isSignedIn = useAppSession() !== null;
  const section = sectionForPath(pathname);

  return (
    <VStack gap={0} as="header">
      {back}
      <HStack align="end" gap={6} justify="between" wrap="wrap" xstyle={styles.head}>
        <VStack gap={2} xstyle={styles.titleBlock}>
          {/* Focus lands here after a route change (`#/ui/arrivalFocus.ts`). */}
          <Heading level={1} tabIndex={-1} xstyle={styles.title}>
            {title}
            {figure}
          </Heading>
          {description ? (
            <Text color="secondary" type="large" xstyle={styles.description}>
              {description}
            </Text>
          ) : null}
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
  // The standard and wide measures hold their gutters, so at 1440 a standard
  // page's content starts at x=132.
  standard: {
    maxInlineSize: pageLayout.standardWidth,
  },
  wide: {
    maxInlineSize: pageLayout.wideWidth,
  },
  narrow: {
    maxInlineSize: `calc(${pageLayout.narrowWidth} + 2 * ${pageLayout.gutterDesktop})`,
  },
  full: {
    paddingInlineStart: 0,
    paddingInlineEnd: 0,
    paddingBlock: 0,
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
  description: {
    maxInlineSize: "60ch",
  },
  // Sub-pages are underline tabs on a 1px rule. On a phone the strip bleeds to
  // the screen edges and scrolls sideways rather than wrapping.
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

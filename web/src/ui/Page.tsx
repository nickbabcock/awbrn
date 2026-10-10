import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { RouterTextLink } from "#/ui/astryx-links.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { sectionForPath, type SiteSection } from "#/ui/site_sections.ts";

type PageWidth = "standard" | "narrow" | "full";

/**
 * The frame of one screen: a centered measure on the terrain with one gutter.
 *
 * Every page sits in the same frame, so the title, the first panel, and the
 * edge of the content land in the same place on every screen. A board that
 * needs the whole window asks for `full`; prose and single forms ask for
 * `narrow`.
 */
export function Page({ children, width = "standard" }: { children: ReactNode; width?: PageWidth }) {
  return (
    <VStack
      gap={8}
      xstyle={[
        styles.frame,
        width === "standard" && styles.standard,
        width === "narrow" && styles.narrow,
      ]}
    >
      {children}
    </VStack>
  );
}

/**
 * The head of a page: its title, one line on what it is for, its commands, and
 * the other pages of its section.
 *
 * The title is the same size on every page. A page that is a section of the
 * site gets the section's tabs under its title, so a player can see where they
 * are and move sideways without going back to the bar.
 */
export function PageHeader({
  actions,
  description,
  title,
}: {
  actions?: ReactNode;
  description?: ReactNode;
  title: ReactNode;
}) {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const isSignedIn = useAppSession() !== null;
  const section = sectionForPath(pathname);

  return (
    <VStack gap={4} as="header">
      <HStack align="end" gap={4} justify="between" wrap="wrap">
        <VStack gap={2} xstyle={styles.titleBlock}>
          <Heading level={1}>{title}</Heading>
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
    <HStack as="nav" aria-label={section.label} gap={0} wrap="wrap" xstyle={styles.tabs}>
      {pages.map((page) => (
        <RouterTextLink
          activeOptions={{ exact: true }}
          aria-current={page.to === selected ? "page" : undefined}
          isStandalone
          key={page.to}
          to={page.to}
          xstyle={[styles.tab, page.to === selected && styles.tabSelected]}
        >
          {page.label}
        </RouterTextLink>
      ))}
    </HStack>
  );
}

const styles = stylex.create({
  frame: {
    boxSizing: "border-box",
    inlineSize: "100%",
    marginInline: "auto",
    paddingInline: {
      default: spacingVars["--spacing-8"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
    paddingBlockStart: {
      default: spacingVars["--spacing-8"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-5"],
    },
    paddingBlockEnd: spacingVars["--spacing-10"],
  },
  standard: {
    maxInlineSize: `calc(${pageLayout.standardWidth} + 2 * ${spacingVars["--spacing-8"]})`,
  },
  narrow: {
    maxInlineSize: `calc(${pageLayout.narrowWidth} + 2 * ${spacingVars["--spacing-8"]})`,
  },
  titleBlock: {
    minInlineSize: 0,
    flexBasis: "32rem",
    flexGrow: 1,
  },
  // A description is one sentence, and a sentence stretched across a desktop
  // window is read as two separate lines of text.
  description: {
    maxInlineSize: "60ch",
  },
  // The tabs sit on the sky, where the rail under them is the only edge the
  // strip has. It takes the one ink the rest of the chrome is drawn in.
  tabs: {
    borderColor: colorVars["--color-border-emphasized"],
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  // A tab is a HUD label, the same voice as the bar above it, so the two
  // levels of navigation read as one system.
  tab: {
    paddingBlock: spacingVars["--spacing-3"],
    paddingInline: spacingVars["--spacing-4"],
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
  },
  tabSelected: {
    color: colorVars["--color-text-primary"],
  },
});

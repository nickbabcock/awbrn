import { Icon } from "@astryxdesign/core/Icon";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { borderVars, spacingVars, typeScaleVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { CountBadge } from "#/ui/CountBadge.tsx";
import { IconHome, type DrawnIcon } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { shellVars } from "#/ui/shell.stylex.ts";
import { SITE_SECTIONS, sectionForPath } from "#/ui/site_sections.ts";

interface Tab {
  id: string;
  label: string;
  icon: DrawnIcon;
  to: string;
  isCurrent: boolean;
  count: number;
}

/**
 * The phone's 64px bottom tab bar: Home, Play, My games, Maps, Replays
 * (signed out: Play, Maps, Replays). It is fixed to the bottom edge, pads the
 * home indicator, and steps aside on pushed and task screens.
 */
export function TabBar({
  awaiting,
  isSignedIn,
  pathname,
}: {
  awaiting: number;
  isSignedIn: boolean;
  pathname: string;
}) {
  const current = sectionForPath(pathname);
  const sections = SITE_SECTIONS.filter((section) => isSignedIn || !section.requiresSession);
  const tabs: Tab[] = [
    ...(isSignedIn
      ? [
          {
            id: "home",
            label: "Home",
            icon: IconHome,
            to: "/",
            isCurrent: pathname === "/",
            count: 0,
          },
        ]
      : []),
    ...sections.map((section) => ({
      id: section.id,
      label: section.label,
      icon: section.icon,
      to: section.to,
      isCurrent: current?.id === section.id,
      count: section.id === "mine" ? awaiting : 0,
    })),
  ];

  return (
    <nav aria-label="Main navigation" {...stylex.props(styles.bar, styles.columns(tabs.length))}>
      {tabs.map((tab) => (
        <Link
          aria-current={tab.isCurrent ? "page" : undefined}
          key={tab.id}
          to={tab.to}
          {...stylex.props(styles.tab, tab.isCurrent && styles.current)}
        >
          <Icon icon={tab.icon} size="lg" xstyle={styles.icon} />
          {tab.label}
          {tab.count > 0 ? (
            <>
              <CountBadge count={tab.count} xstyle={styles.count} />
              <VisuallyHidden>
                {tab.count === 1 ? "1 match waits on you" : `${tab.count} matches wait on you`}
              </VisuallyHidden>
            </>
          ) : null}
        </Link>
      ))}
    </nav>
  );
}

const styles = stylex.create({
  columns: (count: number) => ({
    gridTemplateColumns: `repeat(${count}, minmax(0, 1fr))`,
  }),
  bar: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: shellVars.tabBarDisplay,
    },
    position: "fixed",
    insetInline: 0,
    insetBlockEnd: 0,
    zIndex: 40,
    blockSize: `calc(${pageLayout.tabBarHeight} + env(safe-area-inset-bottom, 0px))`,
    paddingBlockEnd: "env(safe-area-inset-bottom, 0px)",
    paddingInline: "env(safe-area-inset-left, 0px) env(safe-area-inset-right, 0px)",
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink950,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  tab: {
    position: "relative",
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    gap: spacingVars["--spacing-1"],
    minInlineSize: 0,
    color: awbrnVars.textOnInkSecondary,
    textDecoration: "none",
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.textNav,
    fontWeight: typeScaleVars["--text-label-weight"],
    whiteSpace: "nowrap",
  },
  current: {
    color: awbrnVars.textOnInkStrong,
    "::before": {
      content: '""',
      position: "absolute",
      insetBlockStart: 0,
      insetInline: "22%",
      blockSize: awbrnVars.currentMark,
      backgroundColor: awbrnVars.textOnInkStrong,
      borderEndStartRadius: awbrnVars.currentMark,
      borderEndEndRadius: awbrnVars.currentMark,
    },
  },
  icon: {
    inlineSize: "22px",
    blockSize: "22px",
  },
  count: {
    position: "absolute",
    insetBlockStart: spacingVars["--spacing-1-5"],
    insetInlineStart: `calc(50% + ${spacingVars["--spacing-1-5"]})`,
    blockSize: "18px",
    minInlineSize: "18px",
    fontSize: awbrnVars.textMicro,
  },
});

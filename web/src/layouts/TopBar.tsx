import { Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import type { Session } from "#/auth/session.ts";
import { AccountMenu } from "#/layouts/AccountMenu.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { AwaitingCount } from "#/ui/CountBadge.tsx";
import { IconPlus } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { shellVars } from "#/ui/shell.stylex.ts";
import { SITE_SECTIONS, pageOffersNewMatch, sectionForPath } from "#/ui/site_sections.ts";

export function TopBar({
  awaiting,
  pathname,
  session,
}: {
  awaiting: number;
  pathname: string;
  session: Session | null;
}) {
  const sections = SITE_SECTIONS.filter((section) => session !== null || !section.requiresSession);
  const current = sectionForPath(pathname);
  const isHome = pathname === "/";
  const showsNewMatch = session !== null && !pageOffersNewMatch(pathname);

  return (
    <header {...stylex.props(styles.bar)}>
      <Link
        aria-current={isHome ? "page" : undefined}
        aria-label="AWBRN home"
        to="/"
        {...stylex.props(styles.brand, isHome && styles.current, isHome && styles.brandCurrent)}
      >
        <Stack aria-hidden="true" as="span" xstyle={styles.brandMark} />
        AWBRN
      </Link>
      <nav aria-label="Main navigation" {...stylex.props(styles.nav)}>
        {sections.map((section) => {
          const isCurrent = current?.id === section.id;
          return (
            <Link
              aria-current={isCurrent ? "page" : undefined}
              key={section.id}
              to={section.to}
              {...stylex.props(styles.navItem, isCurrent && styles.current)}
            >
              <Icon icon={section.icon} size="sm" xstyle={styles.navIcon} />
              {section.label}
              {section.id === "mine" ? <AwaitingCount count={awaiting} /> : null}
            </Link>
          );
        })}
      </nav>
      <Stack as="span" direction="horizontal" xstyle={styles.end}>
        {showsNewMatch ? (
          // The slot's display is a shell variable ("contents" or "none"),
          // which `xstyle`'s typed display cannot take; the class can.
          <Stack as="span" {...stylex.props(styles.newMatchSlot)}>
            <RouterButton
              icon={<Icon icon={IconPlus} size="sm" />}
              label="New match"
              size="sm"
              to="/matches/new"
              variant="ghost-on-ink"
              xstyle={styles.newMatch}
            />
          </Stack>
        ) : null}
        {session ? (
          <AccountMenu session={session} />
        ) : pathname.startsWith("/auth") ? null : (
          <RouterButton
            label="Sign in"
            search={{ mode: undefined }}
            size="sm"
            to="/auth"
            variant="on-ink"
          />
        )}
      </Stack>
    </header>
  );
}

const styles = stylex.create({
  bar: {
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: shellVars.topBarPhoneDisplay,
    },
    alignItems: "center",
    gap: {
      default: "28px",
      [pageLayout.tabletMedia]: "20px",
      [pageLayout.phoneMedia]: "12px",
    },
    inlineSize: "100%",
    blockSize: {
      default: pageLayout.topBarHeight,
      [pageLayout.phoneMedia]: pageLayout.topBarHeightPhone,
    },
    paddingInline: {
      default: "24px",
      [pageLayout.phoneMedia]: pageLayout.gutterPhone,
    },
    backgroundColor: awbrnVars.ink900,
    borderBlockEndColor: awbrnVars.ink950,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
    color: awbrnVars.textOnInk,
  },
  brand: {
    position: "relative",
    alignSelf: "stretch",
    display: "inline-flex",
    alignItems: "center",
    gap: "10px",
    flexShrink: 0,
    color: awbrnVars.textOnInk,
    textDecoration: "none",
    fontFamily: awbrnVars.fontUi,
    fontSize: {
      default: "22px",
      [pageLayout.phoneMedia]: "21px",
    },
    fontWeight: 900,
    fontStretch: "62%",
    letterSpacing: "0.04em",
  },
  brandMark: {
    inlineSize: "22px",
    blockSize: "14px",
    borderRadius: "2px",
    backgroundImage: awbrnVars.band,
    backgroundSize: "30px 30px",
    boxShadow: `inset 0 0 0 1.5px ${awbrnVars.textOnInkStrong}`,
  },
  nav: {
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "none",
    },
    alignItems: "stretch",
    alignSelf: "stretch",
    gap: "2px",
  },
  navItem: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    paddingInline: "12px",
    color: {
      default: awbrnVars.textOnInkSecondary,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.textOnInk },
      ":active": awbrnVars.textOnInk,
    },
    textDecoration: "none",
    fontSize: awbrnVars.text14_5,
    fontWeight: 600,
    whiteSpace: "nowrap",
    transitionProperty: "color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
  },
  // Signed in, the full bar needs about 830px. Below the rail width the
  // section labels stand alone, so the bar fits from 721px, and on a notched
  // phone held sideways inside its insets.
  navIcon: {
    display: {
      default: null,
      [pageLayout.belowRailMedia]: "none",
    },
  },
  current: {
    color: awbrnVars.textOnInkStrong,
    "::after": {
      content: '""',
      position: "absolute",
      insetInline: "10px",
      insetBlockEnd: 0,
      blockSize: awbrnVars.currentMark,
      backgroundColor: awbrnVars.textOnInkStrong,
      borderStartStartRadius: awbrnVars.currentMark,
      borderStartEndRadius: awbrnVars.currentMark,
    },
  },
  brandCurrent: {
    "::after": {
      insetInline: 0,
    },
  },
  end: {
    display: "flex",
    alignItems: "center",
    gap: "12px",
    marginInlineStart: "auto",
  },
  newMatchSlot: {
    display: {
      default: shellVars.newMatchDisplay,
      [pageLayout.phoneMedia]: "none",
    },
  },
  newMatch: {
    borderColor: awbrnVars.ink600,
    color: awbrnVars.textOnInkStrong,
  },
});

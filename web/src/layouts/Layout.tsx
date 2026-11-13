import { useQuery } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { AppShell } from "@astryxdesign/core/AppShell";
import { VStack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { matchesAwaitingQueryOptions } from "#/matches/matches.queries.ts";
import { usePlayerNotifications, useTabBadge } from "#/players/player_notifications.ts";
import { SiteFooter } from "#/layouts/SiteFooter.tsx";
import { TabBar } from "#/layouts/TabBar.tsx";
import { TopBar } from "#/layouts/TopBar.tsx";
import { useRouteShellMode } from "#/layouts/useRouteShellMode.ts";
import { ShellMode } from "#/ui/ShellMode.tsx";
import { useArrivalFocus } from "#/ui/arrivalFocus.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { shellMarker, shellVars } from "#/ui/shell.stylex.ts";

/**
 * The frame around every screen: the navy top bar, the page, the independence
 * footer and, on a phone, the tab bar.
 *
 * A screen that is not a paper page says so with `<ShellMode>` (see
 * `#/ui/ShellMode.tsx`); the root reads that marker with `:has()` and hides the
 * chrome that screen does not want, in the first paint.
 */
export function Layout({ children }: { children: ReactNode }) {
  const session = useAppSession();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  // The player's own socket, which every match reports a turn change to. It is
  // what re-reads the count below, so nothing here has to guess how often
  // whose turn it is might have moved.
  const socketStatus = usePlayerNotifications(session !== null);
  // Matches that wait on this player, ranked pairings to confirm included.
  // It is the one count badge, on My games.
  const { data: awaitingData } = useQuery({
    ...matchesAwaitingQueryOptions(socketStatus === "connected"),
    enabled: session !== null,
  });
  const awaiting = session !== null ? (awaitingData?.awaiting ?? 0) : 0;
  // The same number in the browser tab, which is all a player reading another
  // tab can see of the site.
  useTabBadge(awaiting, pathname);
  const routeShellMode = useRouteShellMode();
  // A route change moves focus to the new page's title (or a pushed screen's
  // turn line).
  useArrivalFocus();

  return (
    <VStack gap={0} xstyle={styles.root}>
      {routeShellMode ? <ShellMode mode={routeShellMode} /> : null}
      <AppShell
        contentPadding={0}
        height="auto"
        mobileNav={false}
        topNav={<TopBar awaiting={awaiting} pathname={pathname} session={session} />}
        variant="surface"
      >
        <VStack gap={0} xstyle={styles.body}>
          <VStack gap={0} xstyle={styles.content}>
            {children}
          </VStack>
          <SiteFooter />
        </VStack>
      </AppShell>
      <TabBar awaiting={awaiting} isSignedIn={session !== null} pathname={pathname} />
    </VStack>
  );
}

const styles = stylex.create({
  // The AppShell under it is at least the window tall (Astryx's own
  // `min-height: 100dvh`), so the root adds no height of its own.
  root: {
    [shellVars.tabBarDisplay]: {
      default: "grid",
      [stylex.when.descendant('[data-shell-mode="pushed"]', shellMarker)]: "none",
      [stylex.when.descendant('[data-shell-mode="task"]', shellMarker)]: "none",
    },
    [shellVars.topBarPhoneDisplay]: {
      default: "flex",
      [stylex.when.descendant('[data-shell-mode="pushed"]', shellMarker)]: "none",
    },
    [shellVars.footerDisplay]: {
      default: "flex",
      [stylex.when.descendant('[data-shell-mode="pushed"]', shellMarker)]: "none",
    },
    [shellVars.newMatchDisplay]: {
      default: "contents",
      [stylex.when.descendant('[data-shell-mode="pushed"]', shellMarker)]: "none",
    },
    [shellVars.tabBarSpace]: {
      default: pageLayout.tabBarHeight,
      [stylex.when.descendant('[data-shell-mode="pushed"]', shellMarker)]: "0px",
      [stylex.when.descendant('[data-shell-mode="task"]', shellMarker)]: "0px",
    },
  },
  // The page grows to the window, so a short page still puts its foot at the
  // bottom of the screen, and the document is exactly the window tall.
  // AppShell's main is not a flex box, so the page cannot simply grow into
  // it: it takes the window less what AppShell sets around it, the top bar
  // with the top inset above it and the bottom inset under main.
  body: {
    boxSizing: "border-box",
    inlineSize: "100%",
    minInlineSize: 0,
    minBlockSize: {
      default: `calc(100dvh - ${pageLayout.topBarHeight} - env(safe-area-inset-top, 0px) - env(safe-area-inset-bottom, 0px))`,
      [pageLayout.phoneMedia]: `calc(100dvh - ${pageLayout.topBarHeightPhone} - env(safe-area-inset-top, 0px) - env(safe-area-inset-bottom, 0px))`,
    },
    // The fixed tab bar covers the bottom of the page on a phone; the page
    // leaves it that much room, inside its own height.
    paddingBlockEnd: {
      default: 0,
      [pageLayout.phoneMedia]: shellVars.tabBarSpace,
    },
  },
  content: {
    flexGrow: 1,
    minInlineSize: 0,
  },
});

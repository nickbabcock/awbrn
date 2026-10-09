import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { AppShell } from "@astryxdesign/core/AppShell";
import { Badge } from "@astryxdesign/core/Badge";
import { DropdownMenu } from "@astryxdesign/core/DropdownMenu";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TopNav } from "@astryxdesign/core/TopNav";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import type { ReactNode } from "react";
import { useState } from "react";
import { authClient } from "#/auth/client.ts";
import { authKeys } from "#/auth/auth.keys.ts";
import { useAppSession } from "#/auth/useAppSession.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchesAwaitingQueryOptions } from "#/matches/matches.queries.ts";
import { rankedKeys } from "#/matchmaking/matchmaking.keys.ts";
import { rankedOverviewQueryOptions } from "#/matchmaking/matchmaking.queries.ts";
import { usePlayerNotifications, useTabBadge } from "#/players/player_notifications.ts";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import {
  RouterButton,
  RouterTextLink,
  RouterTopNavHeading,
  RouterTopNavItem,
} from "#/ui/astryx-links.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { SITE_SECTIONS, sectionForPath, type SiteSection } from "#/ui/site_sections.ts";

export function Layout({ children }: { children: ReactNode }) {
  const session = useAppSession();
  const navigate = useNavigate();
  const router = useRouter();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const queryClient = useQueryClient();
  // The badge is the only announcement a pairing gets. It reports pairings
  // that are waiting for this player, and nothing about the pool.
  const { data: ranked } = useQuery({ ...rankedOverviewQueryOptions(), enabled: session !== null });
  const pendingPairings =
    ranked?.pools.reduce((total, pool) => total + pool.pending.length, 0) ?? 0;
  // The player's own socket, which every match reports a turn change to. It is
  // what re-reads the counts below, so nothing here has to guess how often
  // whose turn it is might have moved.
  const socketStatus = usePlayerNotifications(session !== null);
  const { data: awaitingData } = useQuery({
    ...matchesAwaitingQueryOptions(socketStatus === "connected"),
    enabled: session !== null,
  });
  const awaiting = awaitingData?.awaiting ?? 0;
  // Every badge on the page collapses to this one number in the tab, which is
  // all a player reading another tab can see of the site. It counts matches
  // rather than badges, so a match that two badges name is still one thing to
  // come back for.
  useTabBadge(session !== null ? awaiting : 0, pathname);
  const [isSigningOut, setIsSigningOut] = useState(false);
  const [signOutError, setSignOutError] = useState<string | null>(null);

  async function handleSignOut() {
    if (isSigningOut) {
      return;
    }

    setIsSigningOut(true);
    setSignOutError(null);

    try {
      const result = await authClient.signOut();

      if (result.error) {
        setSignOutError(result.error.message ?? "Sign out failed");
        return;
      }

      queryClient.removeQueries({ queryKey: matchKeys.mine() });
      queryClient.removeQueries({ queryKey: matchKeys.awaiting() });
      queryClient.removeQueries({ queryKey: rankedKeys.all });
      queryClient.removeQueries({ queryKey: matchKeys.completed() });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: authKeys.all }),
        queryClient.invalidateQueries({ queryKey: matchKeys.details() }),
      ]);
      await router.invalidate();
      await navigate({ to: "/" });
    } catch (error) {
      setSignOutError(error instanceof Error ? error.message : "Sign out failed");
    } finally {
      setIsSigningOut(false);
    }
  }

  // The bar names the four places a player goes. The pages inside each place
  // are tabs under the page title, so the bar stays short enough to read in
  // one glance and New match stays a command rather than a place.
  const sections = SITE_SECTIONS.filter((section) => session !== null || !section.requiresSession);
  const current = sectionForPath(pathname);
  // Each count stays on the section that holds the page it is about: a
  // pairing to confirm is on Ranked, which is part of Play, and a turn to take
  // is on My games.
  const sectionBadges: Partial<Record<SiteSection["id"], { count: number; label: string }>> =
    session === null
      ? {}
      : {
          play: {
            count: pendingPairings,
            label:
              pendingPairings === 1
                ? "1 pairing needs you"
                : `${pendingPairings} pairings need you`,
          },
          mine: {
            count: awaiting,
            label: awaiting === 1 ? "1 game awaits your turn" : `${awaiting} games await your turn`,
          },
        };

  const topNav = (
    <TopNav
      label="Main navigation"
      heading={<RouterTopNavHeading heading="AWBRN" to="/" />}
      startContent={
        <>
          {sections.map((section) => {
            const badge = sectionBadges[section.id];
            return (
              <RouterTopNavItem
                isSelected={current?.id === section.id}
                key={section.id}
                label={section.label}
                to={section.to}
              >
                {badge && badge.count > 0 ? (
                  <HStack align="center" gap={1}>
                    <Text type="inherit">{section.label}</Text>
                    <Badge label={badge.count} variant="warning" />
                    {/* The badge reads as a bare number aloud, so what the
                        number is about is said here instead. */}
                    <VisuallyHidden>{badge.label}</VisuallyHidden>
                  </HStack>
                ) : null}
              </RouterTopNavItem>
            );
          })}
        </>
      }
      endContent={
        <HStack align="center" gap={2}>
          <RouterButton
            label="New match"
            size="sm"
            to="/matches/new"
            variant="secondary"
            xstyle={styles.desktopOnly}
          />
          {session ? (
            <>
              {signOutError ? (
                <Text color="primary" role="alert" type="supporting">
                  {signOutError}
                </Text>
              ) : null}
              <DropdownMenu
                alignment="end"
                button={{
                  isLoading: isSigningOut,
                  label: session.user.name,
                  size: "sm",
                  variant: "ghost",
                }}
                items={[
                  {
                    label: isSigningOut ? "Signing out" : "Sign out",
                    onClick: () => void handleSignOut(),
                  },
                ]}
                menuWidth={180}
              />
            </>
          ) : (
            <>
              <RouterButton
                to="/auth"
                search={{ mode: undefined }}
                label="Sign in"
                size="sm"
                variant="secondary"
              />
              <RouterButton
                to="/auth"
                search={{ mode: "register" }}
                label="Register"
                size="sm"
                variant="primary"
              />
            </>
          )}
        </HStack>
      }
    />
  );

  return (
    <AppShell contentPadding={0} height="auto" topNav={topNav} variant="wash">
      <VStack gap={0} xstyle={styles.body}>
        <VStack gap={0} xstyle={styles.content}>
          {children}
        </VStack>
        <SiteFooter />
      </VStack>
    </AppShell>
  );
}

/**
 * The foot of every page: what AWBRN is, said once, and the way to About.
 *
 * AWBRN is independent, and the foot is where that is said, so no screen has
 * to make room for it and no screen can be read as implying otherwise.
 */
function SiteFooter() {
  return (
    <HStack align="center" as="footer" gap={4} justify="between" wrap="wrap" xstyle={styles.footer}>
      <Text type="supporting">
        AWBRN is an independent client for Advance Wars By Web players. It is not affiliated with
        AWBW, Nintendo, or Intelligent Systems.
      </Text>
      <HStack align="center" gap={4}>
        <RouterTextLink to="/about">About AWBRN</RouterTextLink>
      </HStack>
    </HStack>
  );
}

const styles = stylex.create({
  // The page grows to the window, so a short page still puts its foot at the
  // bottom of the screen rather than halfway up the terrain.
  body: {
    inlineSize: "100%",
    minInlineSize: 0,
    minBlockSize: "calc(100dvh - 3.5rem)",
  },
  content: {
    flexGrow: 1,
    minInlineSize: 0,
  },
  footer: {
    borderTopColor: colorVars["--color-border-emphasized"],
    borderTopStyle: "solid",
    borderTopWidth: "var(--border-width)",
    backgroundColor: colorVars["--color-background-surface"],
    paddingBlock: spacingVars["--spacing-4"],
    paddingInline: {
      default: spacingVars["--spacing-8"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
  },
  // On a phone the bar has room for the account and the menu, and New match
  // is already the third tab of Play.
  desktopOnly: {
    display: {
      default: null,
      [pageLayout.phoneMedia]: "none",
    },
  },
});

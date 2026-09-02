import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { AppShell } from "@astryxdesign/core/AppShell";
import { Badge } from "@astryxdesign/core/Badge";
import { Button } from "#/ui/Button.tsx";
import { HStack } from "@astryxdesign/core/Stack";
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
import { PushToggle } from "#/players/components/PushToggle.tsx";
import { usePlayerNotifications, useTabBadge } from "#/players/player_notifications.ts";
import { RouterButton, RouterTopNavHeading, RouterTopNavItem } from "#/ui/astryx-links.tsx";

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

  const topNav = (
    <TopNav
      label="Main navigation"
      heading={<RouterTopNavHeading heading="AWBRN" to="/" />}
      startContent={
        <>
          <RouterTopNavItem to="/" isSelected={pathname === "/"} label="Play" />
          <RouterTopNavItem
            to="/matches"
            isSelected={
              pathname === "/matches" ||
              pathname === "/matches/" ||
              (pathname !== "/matches/new" && /^\/matches\/[^/]+\/?$/.test(pathname))
            }
            label="Matches"
          />
          <RouterTopNavItem
            to="/maps"
            isSelected={pathname === "/maps" || pathname.startsWith("/maps/")}
            label="Maps"
          />
          {session ? (
            <>
              <RouterTopNavItem to="/ranked" isSelected={pathname === "/ranked"} label="Ranked">
                <HStack align="center" gap={1}>
                  <Text type="inherit">Ranked</Text>
                  {pendingPairings > 0 ? (
                    <>
                      <Badge label={pendingPairings} variant="warning" />
                      {/* The badge reads as a bare number aloud, so what the
                          number is about is said here instead. */}
                      <VisuallyHidden>
                        {pendingPairings === 1
                          ? "1 pairing needs you"
                          : `${pendingPairings} pairings need you`}
                      </VisuallyHidden>
                    </>
                  ) : null}
                </HStack>
              </RouterTopNavItem>
              <RouterTopNavItem
                to="/my/matches"
                isSelected={pathname === "/my/matches"}
                label="My Matches"
              >
                <HStack align="center" gap={1}>
                  <Text type="inherit">My Matches</Text>
                  {awaiting > 0 ? (
                    <>
                      <Badge label={awaiting} variant="warning" />
                      <VisuallyHidden>
                        {awaiting === 1
                          ? "1 game awaits your turn"
                          : `${awaiting} games await your turn`}
                      </VisuallyHidden>
                    </>
                  ) : null}
                </HStack>
              </RouterTopNavItem>
              <RouterTopNavItem
                to="/my/history"
                isSelected={pathname === "/my/history"}
                label="History"
              />
            </>
          ) : null}
          <RouterTopNavItem
            to="/matches/new"
            isSelected={pathname === "/matches/new"}
            label="New Match"
          />
          <RouterTopNavItem to="/about" isSelected={pathname === "/about"} label="About" />
        </>
      }
      endContent={
        <HStack align="center" gap={1} wrap="wrap">
          {session ? (
            <>
              <Text color="secondary" type="supporting">
                {session.user.name}
              </Text>
              <PushToggle isSignedIn />
              {signOutError ? (
                <Text color="primary" role="alert" type="supporting">
                  {signOutError}
                </Text>
              ) : null}
              <Button
                clickAction={handleSignOut}
                isLoading={isSigningOut}
                label={isSigningOut ? "Signing out" : "Sign out"}
                size="sm"
                variant="secondary"
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
      {children}
    </AppShell>
  );
}

import { useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { AppShell } from "@astryxdesign/core/AppShell";
import { Button } from "#/ui/Button.tsx";
import { HStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TopNav } from "@astryxdesign/core/TopNav";
import type { ReactNode } from "react";
import { useState } from "react";
import { authClient } from "#/auth/client.ts";
import { authKeys } from "#/auth/auth.keys.ts";
import { useAppSession } from "#/auth/useAppSession.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { RouterButton, RouterTopNavHeading, RouterTopNavItem } from "#/ui/astryx-links.tsx";

export function Layout({ children }: { children: ReactNode }) {
  const session = useAppSession();
  const navigate = useNavigate();
  const router = useRouter();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const queryClient = useQueryClient();
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
              <RouterTopNavItem
                to="/my/matches"
                isSelected={pathname === "/my/matches"}
                label="My Matches"
              />
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

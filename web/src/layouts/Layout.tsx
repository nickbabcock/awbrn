import { useQueryClient } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { Link } from "@tanstack/react-router";
import { useNavigate, useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { authClient } from "#/auth/client.ts";
import { authKeys } from "#/auth/auth.keys.ts";
import { useAppSession } from "#/auth/useAppSession.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { Button, ButtonLink, Text, Wordmark } from "#/ui/primitives.tsx";
import { media, tokens } from "#/ui/theme.stylex.ts";

const styles = stylex.create({
  shell: {
    minHeight: "100vh",
    display: "flex",
    flexDirection: "column",
  },
  nav: {
    position: "sticky",
    top: 0,
    zIndex: 10,
    display: "flex",
    justifyContent: "space-between",
    gap: tokens.space4,
    minHeight: tokens.navHeight,
    paddingInline: "clamp(16px, 4vw, 32px)",
    backgroundColor: tokens.chromeBgLight,
    borderBottomWidth: 3,
    borderBottomStyle: "solid",
    borderBottomColor: tokens.strokeHeavy,
    boxShadow: tokens.shadowHardLg,
    flexWrap: {
      default: "nowrap",
      [media.compact]: "wrap",
    },
    alignItems: {
      default: "center",
      [media.compact]: "flex-start",
    },
    paddingBlock: {
      default: null,
      [media.compact]: tokens.space3,
    },
  },
  navCluster: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space3,
    flexWrap: "wrap",
  },
  navLinks: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space1,
    order: {
      default: null,
      [media.compact]: 3,
    },
    width: {
      default: null,
      [media.compact]: "100%",
    },
    justifyContent: {
      default: null,
      [media.compact]: "space-between",
    },
  },
  navLink: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    minHeight: 36,
    paddingInline: tokens.space3,
    borderWidth: 2,
    borderStyle: "solid",
    borderRadius: tokens.radius2,
    color: {
      default: tokens.inkStrong,
      ":hover": tokens.inkStrong,
    },
    fontFamily: tokens.fontPixel,
    fontSize: 9,
    letterSpacing: "0.08em",
    textDecoration: "none",
    textTransform: "uppercase",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "background-color, border-color, color, transform, box-shadow",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
    boxShadow: {
      default: tokens.shadowHardSm,
      ":hover": tokens.shadowHardMd,
      ":active": "none",
    },
    borderColor: {
      default: tokens.strokeBase,
      ":hover": tokens.strokeHeavy,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": tokens.panelBg,
    },
  },
  navLinkActive: {
    backgroundColor: tokens.panelInset,
    borderColor: tokens.strokeHeavy,
    color: tokens.inkStrong,
    boxShadow: tokens.shadowHardMd,
  },
  authZone: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space2,
    flexWrap: "wrap",
    justifyContent: "flex-end",
  },
  user: {
    color: tokens.inkSoft,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
  },
  error: {
    color: tokens.danger,
  },
  navActionSecondary: {
    borderColor: tokens.strokeBase,
    color: tokens.inkStrong,
  },
  navActionPrimary: {
    backgroundColor: tokens.brand,
    borderColor: tokens.strokeHeavy,
    color: tokens.onDarkStrong,
  },
  main: {
    flex: 1,
    minHeight: 0,
  },
});

const navLinkClassName = stylex.props(styles.navLink).className ?? undefined;
const navLinkActiveClassName =
  stylex.props(styles.navLink, styles.navLinkActive).className ?? undefined;

export function Layout({ children }: { children: ReactNode }) {
  const session = useAppSession();
  const navigate = useNavigate();
  const router = useRouter();
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

  return (
    <div {...stylex.props(styles.shell)}>
      <header {...stylex.props(styles.nav)}>
        <div {...stylex.props(styles.navCluster)}>
          <Wordmark href="/" size="nav" shadow />
        </div>

        <nav aria-label="Main navigation" {...stylex.props(styles.navLinks)}>
          <Link
            to="/"
            className={navLinkClassName}
            activeProps={{ className: navLinkActiveClassName }}
            activeOptions={{ exact: true }}
          >
            Play
          </Link>
          <Link
            to="/matches"
            className={navLinkClassName}
            activeProps={{ className: navLinkActiveClassName }}
            activeOptions={{ exact: true }}
          >
            Matches
          </Link>
          {session ? (
            <Link
              to="/my/matches"
              className={navLinkClassName}
              activeProps={{ className: navLinkActiveClassName }}
            >
              My Matches
            </Link>
          ) : null}
          <Link
            to="/matches/new"
            className={navLinkClassName}
            activeProps={{ className: navLinkActiveClassName }}
          >
            New Match
          </Link>
          <Link
            to="/about"
            className={navLinkClassName}
            activeProps={{ className: navLinkActiveClassName }}
          >
            About
          </Link>
        </nav>

        <div {...stylex.props(styles.authZone)}>
          {session ? (
            <>
              <span {...stylex.props(styles.user)}>{session.user.name}</span>
              {signOutError ? (
                <Text role="alert" size="sm" tone="danger" xstyle={styles.error}>
                  {signOutError}
                </Text>
              ) : null}
              <Button
                disabled={isSigningOut}
                onClick={() => {
                  void handleSignOut();
                }}
                size="sm"
                tone="neutral"
                variant="outline"
                xstyle={styles.navActionSecondary}
              >
                {isSigningOut ? "Signing Out..." : "Sign Out"}
              </Button>
            </>
          ) : (
            <>
              <ButtonLink
                size="sm"
                search={{ mode: undefined }}
                to="/auth"
                tone="neutral"
                variant="outline"
                xstyle={styles.navActionSecondary}
              >
                Sign In
              </ButtonLink>
              <ButtonLink
                size="sm"
                to="/auth"
                search={{ mode: "register" }}
                tone="brand"
                xstyle={styles.navActionPrimary}
              >
                Register
              </ButtonLink>
            </>
          )}
        </div>
      </header>

      <main {...stylex.props(styles.main)}>{children}</main>
    </div>
  );
}

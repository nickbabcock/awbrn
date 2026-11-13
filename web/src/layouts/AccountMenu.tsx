import { BottomSheet } from "@astryxdesign/core/BottomSheet";
import { Divider } from "@astryxdesign/core/Divider";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { Popover } from "@astryxdesign/core/Popover";
import { VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { authKeys } from "#/auth/auth.keys.ts";
import { authClient } from "#/auth/client.ts";
import type { Session } from "#/auth/session.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { rankedKeys } from "#/matchmaking/matchmaking.keys.ts";
import { PushToggle } from "#/players/components/PushToggle.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { IconChevronDown } from "#/ui/icons.tsx";
import { overlayMotion } from "#/ui/overlayMotion.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { useCompactTouch } from "#/ui/useCompactTouch.ts";

/**
 * The account key (44px): the player's initial, and on a wide screen their name.
 * It holds Turn alerts and Sign out, as a popover on desktop and a bottom
 * sheet on a phone.
 */
export function AccountMenu({ session }: { session: Session }) {
  const navigate = useNavigate();
  const router = useRouter();
  const queryClient = useQueryClient();
  const [isOpen, setIsOpen] = useState(false);
  const [isSigningOut, setIsSigningOut] = useState(false);
  const [signOutError, setSignOutError] = useState<string | null>(null);
  const name = session.user.name;
  const isCompactTouch = useCompactTouch();

  async function handleSignOut() {
    if (isSigningOut) return;
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
      setIsOpen(false);
      await router.invalidate();
      await navigate({ to: "/" });
    } catch (error) {
      setSignOutError(error instanceof Error ? error.message : "Sign out failed");
    } finally {
      setIsSigningOut(false);
    }
  }

  const menu = (
    <VStack gap={3} xstyle={styles.menu}>
      <Heading level={4} xstyle={styles.oneLine}>
        {name}
      </Heading>
      <PushToggle isSignedIn />
      <Divider />
      {signOutError ? (
        <Text color="danger" role="alert" type="supporting">
          {signOutError}
        </Text>
      ) : null}
      <Button
        clickAction={handleSignOut}
        isLoading={isSigningOut}
        label="Sign out"
        variant="secondary"
        width="100%"
      />
    </VStack>
  );

  const trigger = (
    <button
      aria-expanded={isCompactTouch ? isOpen : undefined}
      aria-haspopup="dialog"
      aria-label={`Account: ${name}`}
      onClick={isCompactTouch ? () => setIsOpen(true) : undefined}
      type="button"
      {...stylex.props(styles.key)}
    >
      <Text aria-hidden="true" type="inherit" xstyle={styles.avatar}>
        {initialOf(name)}
      </Text>
      <Text
        aria-hidden="true"
        type="inherit"
        xstyle={[styles.oneLine, styles.keyName, styles.wideOnly]}
      >
        {name}
      </Text>
      <Icon icon={IconChevronDown} size="sm" xstyle={[styles.chevron, styles.wideOnly]} />
    </button>
  );

  // A phone gets a bottom sheet that hugs the menu; a desktop a popover.
  if (isCompactTouch) {
    return (
      <>
        {trigger}
        <BottomSheet
          height="hug"
          isOpen={isOpen}
          label="Account"
          onOpenChange={setIsOpen}
          xstyle={[overlayMotion.sheet, !isOpen && overlayMotion.sheetLeaving]}
        >
          {menu}
        </BottomSheet>
      </>
    );
  }

  return (
    <Popover
      alignment="end"
      content={menu}
      isOpen={isOpen}
      label="Account"
      onOpenChange={setIsOpen}
      padding={4}
      width={260}
    >
      {trigger}
    </Popover>
  );
}

function initialOf(name: string): string {
  return Array.from(name.trim())[0]?.toUpperCase() ?? "?";
}

const styles = stylex.create({
  key: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    minBlockSize: "44px",
    minInlineSize: "44px",
    paddingInline: "4px",
    borderStyle: "none",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.ink850 },
      ":active": awbrnVars.ink850,
    },
    color: awbrnVars.textOnInk,
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text14,
    fontWeight: 600,
    cursor: "pointer",
    transitionProperty: "background-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
  },
  avatar: {
    display: "inline-grid",
    placeItems: "center",
    inlineSize: "32px",
    blockSize: "32px",
    borderRadius: "50%",
    borderColor: awbrnVars.ink600,
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: awbrnVars.ink700,
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text13,
    fontWeight: 700,
  },
  chevron: {
    color: awbrnVars.textOnInkSecondary,
  },
  // Below the rail width the key is the 44px initial alone, so the top bar
  // fits a tablet.
  wideOnly: {
    display: {
      default: null,
      [pageLayout.belowRailMedia]: "none",
    },
  },
  menu: {
    minInlineSize: 0,
  },
  // A long name holds one line and ends in an ellipsis.
  oneLine: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  keyName: {
    maxInlineSize: "160px",
  },
});

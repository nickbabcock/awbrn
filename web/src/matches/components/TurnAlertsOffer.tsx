import { HStack } from "@astryxdesign/core/Stack";
import { radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useState } from "react";
import { matchLayout } from "#/matches/components/matchLayout.stylex.ts";
import { DAY_MS } from "#/matches/match_clock.ts";
import type { MatchClock } from "#/matches/schemas.ts";
import { PushToggle } from "#/players/components/PushToggle.tsx";
import { pushConfigFn } from "#/players/players.functions.ts";
import { usePushSubscriptionState } from "#/players/player_notifications.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";

const OFFERED_KEY = "awbrn.match.turnAlertsOffered";

/** A match played over days, where a player is not at the board for the next turn. */
export function isAsyncClock(clock: MatchClock): boolean {
  return clock.initialMs >= DAY_MS;
}

/**
 * Once per device, after the first End turn in an async match, the tile row
 * offers the Turn alerts switch with OK, like First touch.
 *
 * `offer()` is called when the viewer's End turn is sent. The offer stands
 * only where the switch can work: a browser with push, a deployment that can
 * sign, a player who has not refused and is not already subscribed.
 */
export function useTurnAlertsOffer(isAsync: boolean) {
  const [isAsked, setIsAsked] = useState(false);
  const [isOffered, setIsOffered] = useState(true);
  useEffect(() => {
    try {
      setIsOffered(window.localStorage.getItem(OFFERED_KEY) !== null);
    } catch {
      // A browser that keeps nothing is never offered: the offer could not
      // remember it had been made.
      setIsOffered(true);
    }
  }, []);

  const isWanted = isAsync && isAsked && !isOffered;
  const { data: config } = useQuery({
    queryKey: ["push", "config"],
    queryFn: () => pushConfigFn(),
    enabled: isWanted,
    staleTime: Infinity,
  });
  const { data: state } = usePushSubscriptionState(isWanted);
  const canWork =
    config?.publicKey != null &&
    state !== undefined &&
    state.permission !== "unsupported" &&
    state.permission !== "denied" &&
    state.endpoint === null;

  // Once on screen the offer stays until OK, so turning alerts on shows the
  // switch on rather than taking it away.
  const [isShown, setIsShown] = useState(false);
  useEffect(() => {
    if (isWanted && canWork) setIsShown(true);
  }, [canWork, isWanted]);

  const offer = useCallback(() => setIsAsked(true), []);
  const dismiss = useCallback(() => {
    setIsOffered(true);
    try {
      window.localStorage.setItem(OFFERED_KEY, "1");
    } catch {
      // Nothing to remember it in; the offer is gone for this page.
    }
  }, []);

  return { isOffering: isWanted && (isShown || canWork), offer, dismiss };
}

/** The offer, in the tile row: the Turn alerts switch, then OK. */
export function TurnAlertsOffer({ onDismiss }: { onDismiss: () => void }) {
  return (
    <HStack as="span" role="status" xstyle={styles.offer}>
      <HStack as="span" xstyle={styles.switchCell}>
        <PushToggle isSignedIn />
      </HStack>
      <Button label="OK" onClick={onDismiss} variant="on-ink" xstyle={styles.okKey} />
    </HStack>
  );
}

const styles = stylex.create({
  offer: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    flexGrow: 1,
    minInlineSize: 0,
  },
  switchCell: {
    display: "block",
    flexGrow: 1,
    minInlineSize: 0,
    paddingInline: spacingVars["--spacing-3"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink850,
    color: awbrnVars.textOnInk,
    // The switch is drawn for paper; on ink its quiet words take ink's.
    "--color-text-secondary": awbrnVars.textOnInkSecondary,
  },
  okKey: {
    flexShrink: 0,
    blockSize: matchLayout.target,
    minInlineSize: matchLayout.dockKeyWidth,
  },
});

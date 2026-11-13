import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Switch } from "@astryxdesign/core/Switch";
import { Text } from "@astryxdesign/core/Text";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { IconBell } from "#/ui/icons.tsx";
import { pushConfigFn } from "../players.functions.ts";
import { usePushSubscriptionState } from "../player_notifications.ts";

/**
 * Turn alerts: one switch that lets a player be told about a turn with the
 * site closed. It lives in the account menu and the Match menu.
 *
 * It offers nothing at all unless it can work: a browser without push, a
 * deployment with no signing key, and a player who has refused in the browser
 * itself are each a case where a switch could only fail.
 */
export function PushToggle({ isSignedIn }: { isSignedIn: boolean }) {
  const queryClient = useQueryClient();
  const [isBusy, setIsBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { data: config } = useQuery({
    queryKey: ["push", "config"],
    queryFn: () => pushConfigFn(),
    enabled: isSignedIn,
    staleTime: Infinity,
  });
  const { data: state } = usePushSubscriptionState(isSignedIn);

  if (!isSignedIn || !config?.publicKey || !state || state.permission === "unsupported") {
    return null;
  }

  const isSubscribed = state.endpoint !== null;

  // A player who refused in the browser cannot be asked again from a page, so
  // the row says where the answer now lives instead of offering a dead switch.
  if (state.permission === "denied" && !isSubscribed) {
    return (
      <HStack xstyle={styles.row}>
        <HStack as="span" xstyle={styles.label}>
          <Icon icon={IconBell} size="sm" xstyle={styles.icon} />
          Turn alerts
        </HStack>
        <Text color="secondary" type="supporting">
          Blocked in browser settings
        </Text>
      </HStack>
    );
  }

  async function toggle() {
    setIsBusy(true);
    setError(null);
    try {
      const { disablePush, enablePush } = await import("../push_subscription.ts");
      if (isSubscribed) {
        await disablePush();
      } else {
        await enablePush(config!.publicKey!);
      }
      await queryClient.invalidateQueries({ queryKey: ["push", "subscription"] });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Turn alerts could not change");
    } finally {
      setIsBusy(false);
    }
  }

  return (
    <VStack xstyle={styles.stack}>
      <HStack xstyle={styles.row}>
        <HStack as="span" aria-hidden="true" xstyle={styles.label}>
          <Icon icon={IconBell} size="sm" xstyle={styles.icon} />
          Turn alerts
        </HStack>
        <Switch
          isLabelHidden
          isLoading={isBusy}
          label="Turn alerts"
          onChange={() => void toggle()}
          value={isSubscribed}
        />
      </HStack>
      {error ? (
        <Text color="danger" role="alert" type="supporting">
          {error}
        </Text>
      ) : null}
    </VStack>
  );
}

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: "4px",
  },
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "16px",
    minBlockSize: "44px",
  },
  label: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    fontWeight: 650,
  },
  icon: {
    color: colorVars["--color-text-secondary"],
  },
});

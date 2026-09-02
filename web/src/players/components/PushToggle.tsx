import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "#/ui/Button.tsx";
import { pushConfigFn } from "../players.functions.ts";
import { usePushSubscriptionState } from "../player_notifications.ts";

/**
 * The control that lets a player be told about a turn with the site closed.
 *
 * It offers nothing at all unless it can work: a browser without push, a
 * deployment with no signing key, and a player who has refused in the browser
 * itself are each a case where a button would only be able to fail.
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
  // they are told where the answer now lives instead of given a dead button.
  if (state.permission === "denied" && !isSubscribed) {
    return (
      <Text color="secondary" type="supporting">
        Notifications blocked in browser settings
      </Text>
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
      setError(caught instanceof Error ? caught.message : "Could not change notifications");
    } finally {
      setIsBusy(false);
    }
  }

  return (
    <>
      {error ? (
        <Text color="primary" role="alert" type="supporting">
          {error}
        </Text>
      ) : null}
      <Button
        clickAction={toggle}
        isLoading={isBusy}
        label={isSubscribed ? "Turn off turn notifications" : "Notify me when it is my turn"}
        size="sm"
        variant="secondary"
      />
    </>
  );
}

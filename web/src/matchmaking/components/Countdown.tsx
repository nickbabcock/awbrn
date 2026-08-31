/*
 * A deadline that counts down while the player reads it.
 *
 * The confirmation window is the one clock in ranked play that runs against
 * the player while they decide, so a stale number would be a lie. The
 * component owns its own tick, and it ticks alone: nothing above it re-renders
 * each second.
 */

import { Text } from "@astryxdesign/core/Text";
import { useEffect, useState } from "react";
import { formatCompactDuration } from "#/utils/time.ts";

/** Below this, the window is closing and the number takes the accent color. */
export const URGENT_REMAINDER_MS = 4 * 60 * 60 * 1000;

export function Countdown({
  deadlineAt,
  type = "supporting",
}: {
  deadlineAt: string;
  /** The briefing screen sets the clock at reading size; a row keeps it small. */
  type?: "supporting" | "large";
}) {
  const deadlineMs = Date.parse(deadlineAt);
  const [remaining, setRemaining] = useState(() => deadlineMs - Date.now());

  useEffect(() => {
    setRemaining(deadlineMs - Date.now());
    const timer = setInterval(() => setRemaining(deadlineMs - Date.now()), 1000);
    return () => clearInterval(timer);
  }, [deadlineMs]);

  const isUrgent = remaining <= URGENT_REMAINDER_MS;

  return (
    <Text color={isUrgent ? "accent" : "secondary"} type={type} weight="bold">
      {remaining <= 0 ? "Window closed" : `${formatCompactDuration(remaining)} left`}
    </Text>
  );
}

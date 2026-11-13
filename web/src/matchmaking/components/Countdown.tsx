/*
 * A deadline that counts down while the player reads it.
 *
 * The confirmation window is the one clock in ranked play that runs against
 * the player while they decide, so a stale number would be a lie. The
 * component owns its own tick, and it ticks alone: nothing above it re-renders
 * each second.
 */

import { useEffect, useRef, useState } from "react";
import { clockTickMs } from "#/matches/match_clock.ts";
import { timeLeftFigure, timeLeftLabel } from "#/play/terms.ts";
import { TimeLeft } from "#/ui/TimeLeft.tsx";

export function Countdown({
  deadlineAt,
  isOnInk = false,
  onExpire,
  size = "row",
  tone = "default",
}: {
  deadlineAt: string;
  isOnInk?: boolean;
  /** Called once when the deadline passes, so the page can ask what became of it. */
  onExpire?: () => void;
  size?: "row" | "lead" | "strip";
  /** `urgent` while the window waits on the viewer; `theirs` once it waits on someone else. */
  tone?: "default" | "urgent" | "theirs";
}) {
  const deadlineMs = Date.parse(deadlineAt);
  const [now, setNow] = useState(() => Date.now());
  const remaining = deadlineMs - now;

  const onExpireRef = useRef(onExpire);
  useEffect(() => {
    onExpireRef.current = onExpire;
  });
  const isExpired = remaining <= 0;
  useEffect(() => {
    if (isExpired) onExpireRef.current?.();
  }, [isExpired]);

  useEffect(() => {
    if (remaining <= 0) return;
    const timer = setTimeout(() => setNow(Date.now()), clockTickMs(remaining));
    return () => clearTimeout(timer);
  }, [remaining]);

  return (
    <TimeLeft
      isOnInk={isOnInk}
      label={remaining <= 0 ? "Window closed" : timeLeftLabel(remaining)}
      size={size}
      tone={tone}
      value={timeLeftFigure(remaining)}
    />
  );
}

import { useEffect, useState } from "react";
import { clockTickMs } from "#/matches/match_clock.ts";

/**
 * The wall clock, redrawn only as often as the readout can change.
 *
 * A bank with days on it prints no seconds, so a match left open in a tab
 * costs a frame a minute instead of a frame a second. The interval is read
 * from whatever is nearest to running out, so the last minute of a live match
 * still ticks while a week-long bank does not.
 */
export function useClockNow(nearestRemainingMs: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  const interval = nearestRemainingMs === null ? null : clockTickMs(nearestRemainingMs);

  useEffect(() => {
    if (interval === null) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), interval);
    return () => clearInterval(timer);
  }, [interval]);

  return now;
}

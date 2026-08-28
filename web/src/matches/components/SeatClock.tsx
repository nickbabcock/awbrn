import { HStack, VStack } from "@astryxdesign/core/Stack";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Text } from "@astryxdesign/core/Text";
import { Timestamp } from "@astryxdesign/core/Timestamp";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import {
  clockTickMs,
  clockPressure,
  formatClockCountdown,
  formatClockDuration,
} from "#/matches/match_clock.ts";
import type { MatchClock } from "#/matches/schemas.ts";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { StatIcon } from "#/replay/RosterRow.tsx";

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

/**
 * What the clock leaves one army, as the row reports every other number.
 *
 * The span is set against the game's own clock sprite, which is the same
 * pairing the funds, income, and unit readouts below it already use: the
 * picture says which quantity this is, and the digits say how much. An earlier
 * draft drew a bar instead, and a bar nobody labelled is a box nobody can
 * read.
 *
 * Nothing here moves. The army whose turn is open counts down by redrawing its
 * number; a clock that also pulsed would be a second gesture in a system that
 * has one.
 */
export function SeatClock({
  clock,
  expiresAt,
  isRunning,
  name,
  remaining,
}: {
  clock: MatchClock;
  /**
   * The moment this army runs out, for the one army whose turn is open. Every
   * other bank is standing still and has no moment to name.
   */
  expiresAt: number | null;
  /** True for the seat whose turn is open, and whose bank is being spent. */
  isRunning: boolean;
  /** The army the bank belongs to, for the reading a screen reader gets. */
  name: string;
  remaining: number;
}) {
  const pressure = clockPressure(remaining, clock.initialMs);
  const span = formatClockDuration(remaining);
  const reading =
    remaining <= 0
      ? `${name} is out of time`
      : `${name} has ${span} left${isRunning ? ", and is spending it" : ""}`;
  // The row reads to the nearest hour and redraws at that pace. Opening the
  // clock asks for the seconds, and only then is a seat worth a frame a
  // second, so the fine tick starts when the reading is on screen and stops
  // with it. The tooltip's content stays mounted while it is closed, which is
  // why this is gated on the open state rather than on the content's life.
  const [isOpen, setIsOpen] = useState(false);

  return (
    <Tooltip
      content={<ClockDetail expiresAt={expiresAt} isOpen={isOpen} remaining={remaining} />}
      focusTrigger="always"
      onOpenChange={setIsOpen}
      placement="above"
    >
      {/* The warning is a mark that is simply not there while the bank is
          healthy, so the row never carries a signal a player has to learn to
          ignore. It arrives beside the number the moment it means something. */}
      <HStack align="center" gap={1} tabIndex={0} xstyle={styles.clock}>
        {pressure === "steady" ? null : (
          <StatusDot
            label={pressure === "critical" ? "Nearly out of time" : "Low on time"}
            variant={pressure === "critical" ? "error" : "warning"}
          />
        )}
        {/* The number closes on its sprite the way the readouts below it do.
            It claims no fixed width, though: the stat block reserves one so
            four armies line up in a column, and up here there is no column to
            line up with, only an army's name to leave room for. */}
        <Text hasTabularNumbers maxLines={1} type="label">
          <VisuallyHidden>{reading}</VisuallyHidden>
          <span aria-hidden="true">{span}</span>
        </Text>
        <StatIcon spriteName="Clock.png" />
      </HStack>
    </Tooltip>
  );
}

/**
 * The clock opened rather than glanced at: the seconds, and the moment they
 * run out at.
 *
 * A bank standing still has no such moment, so it says what it is waiting for
 * instead of naming a deadline that would move the next time the turn changes
 * hands.
 */
function ClockDetail({
  expiresAt,
  isOpen,
  remaining,
}: {
  expiresAt: number | null;
  isOpen: boolean;
  remaining: number;
}) {
  const now = useSecondTick(isOpen);
  const left = expiresAt === null ? remaining : Math.max(0, expiresAt - now);

  return (
    <VStack gap={0.5}>
      <Text hasTabularNumbers type="label">
        {left <= 0 ? "Out of time" : formatClockCountdown(left)}
      </Text>
      {expiresAt === null ? (
        <Text color="secondary" type="supporting">
          Not running until this army's turn.
        </Text>
      ) : (
        <Text color="secondary" type="supporting">
          Runs out{" "}
          <Timestamp
            format="date_time"
            hasTooltip={false}
            type="inherit"
            value={new Date(expiresAt).toISOString()}
          />
        </Text>
      )}
    </VStack>
  );
}

/** The wall clock at a second, for as long as somebody is reading it. */
function useSecondTick(isTicking: boolean): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!isTicking) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(timer);
  }, [isTicking]);

  return now;
}

const styles = stylex.create({
  clock: {
    flex: "0 0 auto",
    cursor: "default",
  },
});

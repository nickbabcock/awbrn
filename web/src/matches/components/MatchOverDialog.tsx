import { Text } from "@astryxdesign/core/Text";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { CoStamp } from "#/components/CoStamp.tsx";
import { PaperDialog } from "#/matches/components/EndTurn.tsx";
import type { MatchArmy } from "#/matches/match_armies.ts";
import type { MatchResults, SeatResult } from "#/matches/match_protocol.ts";
import { seatStatus } from "#/matches/match_results.ts";
import { useRatingChange } from "#/players/rating_changes.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconClose } from "#/ui/icons.tsx";
import { Postmark } from "#/ui/Postmark.tsx";
import type { SeatOutcome } from "#/wasm/awbrn_server.js";

/**
 * How a match ended, shown over the board it ended on.
 *
 * A match used to stop without saying so: the last move played out, the clock
 * stopped, and nothing named a winner. This is what says it, and it stands over
 * the final position rather than replacing it, because the position is the
 * thing a player wants to look at while reading the result.
 *
 * The rating is not part of the result and does not wait for one. It is applied
 * after the match by the pool that owns it and announced on the player's own
 * socket, so a rated match reserves the line from the start and its figures
 * fill in when that lands: Watch replay never moves. A match that was never
 * rated has no line.
 */
export function MatchOverDialog({
  armies,
  day,
  isOpen,
  isRated,
  matchId,
  onOpenChange,
  onWatchReplay,
  results,
  viewerSlotIndex,
}: {
  armies: MatchArmy[];
  /** The day the match ended on, for the postmark. */
  day: number | null;
  isOpen: boolean;
  /** Whether the result moves ratings, so the line is held for the figures. */
  isRated: boolean;
  matchId: string;
  onOpenChange: (isOpen: boolean) => void;
  onWatchReplay: () => void;
  results: MatchResults;
  /** The seat the viewer holds, or null when they only watched. */
  viewerSlotIndex: number | null;
}) {
  const ratingChange = useRatingChange(matchId);
  const seats = [...results.seats].sort((left, right) => left.placement - right.placement);
  const viewerSeat =
    viewerSlotIndex === null
      ? null
      : (seats.find((seat) => seat.slotIndex === viewerSlotIndex) ?? null);
  const delta =
    ratingChange === null || ratingChange === undefined
      ? null
      : Math.round(ratingChange.ratingAfter - ratingChange.ratingBefore);

  return (
    <PaperDialog
      footer={<Button label="Watch replay" onClick={onWatchReplay} variant="primary" />}
      headEnd={
        <Button
          icon={<Icon icon={IconClose} />}
          isIconOnly
          label="Close: stay on the board"
          onClick={() => onOpenChange(false)}
          tooltip="Stay on the board"
          variant="secondary"
          xstyle={styles.close}
        />
      }
      headStart={
        day === null ? null : (
          <Postmark figure={day} isNew label={`Ended on day ${day}`} size={56} word="DAY" />
        )
      }
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      title={headline(viewerSeat, seats)}
    >
      <ul {...stylex.props(styles.seats)}>
        {seats.map((seat) => {
          const army = armies.find((candidate) => candidate.entry.playerId === seat.slotIndex);
          const name = army?.name ?? `Seat ${seat.slotIndex + 1}`;
          return (
            <li key={seat.slotIndex} {...stylex.props(styles.seat)}>
              <CoStamp
                coKey={army?.entry.coKey}
                factionCode={army?.entry.displayFactionCode}
                size="sm"
                state={seat.outcome === "loss" ? "canceled" : "default"}
              />
              <HStack as="span" xstyle={styles.seatName}>
                <b>{name}</b>
                {seat.slotIndex === viewerSlotIndex ? (
                  <HStack as="span" xstyle={styles.you}>
                    YOU
                  </HStack>
                ) : null}
                <Text color="inherit" type="inherit" xstyle={styles.reason}>
                  {reasonLabel(seat) || outcomeLabel(seat.outcome)}
                </Text>
              </HStack>
            </li>
          );
        })}
      </ul>
      {viewerSeat && ratingChange && delta !== null ? (
        <HStack as="span" aria-label="Rating" role="group" xstyle={styles.rating}>
          <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.ratingFigures]}>
            {Math.round(ratingChange.ratingBefore)} → {Math.round(ratingChange.ratingAfter)}
          </Text>
          <Text
            color="inherit"
            type="inherit"
            xstyle={[
              figureStyles.figure,
              styles.delta,
              delta > 0 && styles.deltaUp,
              delta < 0 && styles.deltaDown,
            ]}
          >
            {formatDelta(delta)}
          </Text>
        </HStack>
      ) : viewerSeat && isRated ? (
        // The figures' places, as static Paper Well blocks, until the pool
        // reports the rating.
        <HStack as="span" aria-busy="true" aria-label="Rating" role="group" xstyle={styles.rating}>
          <Stack as="span" aria-hidden="true" xstyle={[styles.well, styles.wellFigures]} />
          <Stack as="span" aria-hidden="true" xstyle={[styles.well, styles.wellDelta]} />
        </HStack>
      ) : null}
    </PaperDialog>
  );
}

function headline(viewerSeat: SeatResult | null, seats: SeatResult[]): string {
  if (viewerSeat === null) {
    const winners = seats.filter((seat) => seat.outcome === "win");
    if (winners.length === 0) return "A draw";
    return "Match over";
  }
  switch (viewerSeat.outcome) {
    case "win":
      return "You won";
    case "loss":
      return "You lost";
    case "draw":
      return "A draw";
  }
}

function outcomeLabel(outcome: SeatOutcome): string {
  switch (outcome) {
    case "win":
      return "Won";
    case "loss":
      return "Lost";
    case "draw":
      return "Drew";
  }
}

/** Why this seat ended where it did, in the words the match record uses. */
function reasonLabel(seat: SeatResult): string {
  switch (seat.reason) {
    case undefined:
      return "";
    case "resignation":
      return "Resigned";
    case "timeout":
      return "Clock ran out";
    case "rout":
      return "Routed";
    case "hq-capture":
      return "HQ captured";
    case "lab-capture":
      return "Lab captured";
    case "capture-limit":
      return "Capture limit";
    case "day-limit":
      return "Day limit";
    case "agreement":
      return "Agreed";
    default:
      return seatStatus(seat.reason ?? null);
  }
}

function formatDelta(delta: number): string {
  const rounded = Math.round(delta);
  return rounded >= 0 ? `+${rounded}` : String(rounded);
}

const styles = stylex.create({
  close: {
    inlineSize: "44px",
    minInlineSize: "44px",
    blockSize: "44px",
    paddingInline: 0,
  },
  seats: {
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  seat: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
    minBlockSize: "52px",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  seatName: {
    display: "flex",
    alignItems: "center",
    flexWrap: "wrap",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    fontSize: awbrnVars.text14_5,
  },
  you: {
    display: "inline-flex",
    alignItems: "center",
    blockSize: "18px",
    paddingInline: spacingVars["--spacing-1-5"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.textMicro,
    fontWeight: 750,
    letterSpacing: "0.04em",
  },
  reason: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13,
  },
  rating: {
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    // One height whether the figures are in or still on their way.
    minBlockSize: `calc(${spacingVars["--spacing-3"]} + 28px)`,
    paddingBlockStart: spacingVars["--spacing-3"],
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  well: {
    display: "inline-block",
    blockSize: "22px",
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.paperWell,
  },
  /** `1500 → 1516` at 13.5px mono. */
  wellFigures: {
    inlineSize: "11ch",
  },
  /** `+16` in its chip. */
  wellDelta: {
    inlineSize: "5ch",
  },
  ratingFigures: {
    fontSize: awbrnVars.text13_5,
    fontWeight: 600,
  },
  delta: {
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-1-5"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.paperWell,
    fontSize: awbrnVars.text12_5,
    fontWeight: 600,
  },
  deltaUp: {
    color: awbrnVars.ok,
  },
  deltaDown: {
    color: awbrnVars.danger,
  },
});

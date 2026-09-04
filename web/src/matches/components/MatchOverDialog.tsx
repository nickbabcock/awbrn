import { Badge } from "@astryxdesign/core/Badge";
import { Dialog, DialogHeader } from "@astryxdesign/core/Dialog";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { borderVars, colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import type { CoPortraitCatalog } from "#/components/co_portraits.ts";
import type { MatchArmy } from "#/matches/match_armies.ts";
import { seatStatus } from "#/matches/match_results.ts";
import { useRatingChange } from "#/players/rating_changes.ts";
import { Button } from "#/ui/Button.tsx";
import type { MatchResults, SeatResult } from "#/matches/match_protocol.ts";
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
 * socket, so the line for it is empty until that lands and absent in a match
 * that was never rated.
 */
export function MatchOverDialog({
  armies,
  isOpen,
  matchId,
  onOpenChange,
  onWatchReplay,
  portraitCatalog,
  results,
  viewerSlotIndex,
}: {
  armies: MatchArmy[];
  isOpen: boolean;
  matchId: string;
  onOpenChange: (isOpen: boolean) => void;
  onWatchReplay: () => void;
  portraitCatalog: CoPortraitCatalog;
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

  return (
    <Dialog isOpen={isOpen} onOpenChange={onOpenChange} width={420}>
      <DialogHeader
        onOpenChange={onOpenChange}
        subtitle={subtitle(seats, viewerSeat)}
        title={headline(viewerSeat)}
      />

      <VStack gap={3} padding={3}>
        <VStack gap={0}>
          {seats.map((seat) => {
            const army = armies.find((candidate) => candidate.entry.playerId === seat.slotIndex);
            return (
              <HStack
                align="center"
                gap={2}
                justify="between"
                key={seat.slotIndex}
                paddingBlock={2}
                xstyle={styles.seatRow}
              >
                <HStack align="center" gap={2} xstyle={styles.seatIdentity}>
                  {army?.entry.coKey ? (
                    <CoPortrait
                      catalog={portraitCatalog}
                      coKey={army.entry.coKey}
                      fallbackLabel={army.name}
                      size={24}
                    />
                  ) : null}
                  <Text type="label">{army?.name ?? `Seat ${seat.slotIndex + 1}`}</Text>
                  {seat.slotIndex === viewerSlotIndex ? (
                    <Badge label="You" variant="neutral" />
                  ) : null}
                </HStack>
                <HStack align="center" gap={2}>
                  <Text color="secondary" type="supporting">
                    {reasonLabel(seat)}
                  </Text>
                  <Badge
                    label={outcomeLabel(seat.outcome)}
                    variant={outcomeVariant(seat.outcome)}
                  />
                </HStack>
              </HStack>
            );
          })}
        </VStack>

        {/* Only a seat of the viewer's own can move the viewer's rating, and
            only a rated match moves one at all. */}
        {viewerSeat && ratingChange ? (
          <HStack align="center" gap={2} justify="between" xstyle={styles.ratingRow}>
            <Text type="label">Rating</Text>
            <HStack align="center" gap={2}>
              <Text color="secondary" type="supporting">
                {Math.round(ratingChange.ratingBefore)} → {Math.round(ratingChange.ratingAfter)}
              </Text>
              <Badge
                label={formatDelta(ratingChange.ratingAfter - ratingChange.ratingBefore)}
                variant={ratingVariant(ratingChange.ratingAfter - ratingChange.ratingBefore)}
              />
            </HStack>
          </HStack>
        ) : null}

        <HStack gap={2} justify="end">
          <Button
            clickAction={() => onOpenChange(false)}
            label="Stay on the final board"
            size="sm"
            variant="secondary"
          >
            Stay here
          </Button>
          <Button clickAction={onWatchReplay} label="Watch the replay" size="sm" variant="primary">
            Watch the replay
          </Button>
        </HStack>
      </VStack>
    </Dialog>
  );
}

function headline(viewerSeat: SeatResult | null): string {
  if (viewerSeat === null) return "Match over";
  switch (viewerSeat.outcome) {
    case "win":
      return "You won";
    case "loss":
      return "You lost";
    case "draw":
      return "A draw";
  }
}

/** Who took it, for anybody the result did not name directly. */
function subtitle(seats: SeatResult[], viewerSeat: SeatResult | null): string | undefined {
  if (viewerSeat !== null) return undefined;
  const winners = seats.filter((seat) => seat.outcome === "win");
  if (winners.length === 0) return "Nobody took it.";
  return winners.length === 1 ? "One seat took the match." : "The match was shared.";
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

function outcomeVariant(outcome: SeatOutcome): "success" | "neutral" | "warning" {
  switch (outcome) {
    case "win":
      return "success";
    case "draw":
      return "neutral";
    case "loss":
      return "warning";
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

function ratingVariant(delta: number): "success" | "neutral" | "warning" {
  if (Math.round(delta) > 0) return "success";
  if (Math.round(delta) < 0) return "warning";
  return "neutral";
}

const styles = stylex.create({
  seatRow: {
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border"],
  },
  seatIdentity: {
    minWidth: 0,
  },
  ratingRow: {
    minWidth: 0,
  },
});

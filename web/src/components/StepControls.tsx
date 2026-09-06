import { HStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { ButtonGroup } from "@astryxdesign/core/ButtonGroup";
import { ArrowBarLeft } from "pixelarticons/react/ArrowBarLeft";
import { ArrowBarRight } from "pixelarticons/react/ArrowBarRight";
import { ChevronLeft } from "pixelarticons/react/ChevronLeft";
import { ChevronRight } from "pixelarticons/react/ChevronRight";
import { Button } from "#/ui/Button.tsx";

export interface StepControlsProps {
  /**
   * How far into the game the viewer is standing, or null when the game has
   * not said yet. A live match only counts its actions once somebody asks to
   * read an earlier one.
   */
  position: { index: number; total: number } | null;
  day: number;
  /** Who holds the turn at this moment, or null once the game is over. */
  turnHolder: string | null;
  /** Whether the viewer is standing at the far end of the game. */
  isAtLatest: boolean;
  canStepBack: boolean;
  canStepForward: boolean;
  canStepTurnBack: boolean;
  canStepTurnForward: boolean;
  /**
   * What the far end of the game is called. A match that is still being played
   * ends at "Live"; an archive ends at "End".
   */
  latestLabel: string;
  /** True while the game is answering the last step it was asked for. */
  isBusy?: boolean;
  onSeekStart: () => void;
  onStep: (delta: number) => void;
  onTurnStep: (delta: number) => void;
  onSeekLatest: () => void;
}

/**
 * Walking a game, by actions or by turns.
 *
 * The two step sizes are the two ways a game is read. An action is the unit a
 * fight is read in — a shot, a capture, the move that set one up — and a turn
 * is the unit a game is read in. So both are one press, and neither is a
 * scrub: a player looking for the moment a position turned finds it by
 * crossing turns and then stepping actions, rather than by hunting along a
 * bar.
 *
 * The controls say nothing about where the moment comes from. A live match
 * asks its host and an archive answers itself, and the difference belongs to
 * the page rather than to the row of keys.
 */
export function StepControls({
  position,
  day,
  turnHolder,
  isAtLatest,
  canStepBack,
  canStepForward,
  canStepTurnBack,
  canStepTurnForward,
  latestLabel,
  isBusy = false,
  onSeekStart,
  onStep,
  onTurnStep,
  onSeekLatest,
}: StepControlsProps) {
  return (
    <HStack align="center" gap={3} justify="between" wrap="wrap">
      <ButtonGroup label="Step back" size="sm">
        <Button
          clickAction={onSeekStart}
          icon={<ArrowBarLeft aria-hidden height={16} width={16} />}
          isDisabled={isBusy || !canStepBack}
          isIconOnly
          label="Go to the first turn"
          tooltip="First turn"
          variant="secondary"
        />
        <Button
          clickAction={() => onTurnStep(-1)}
          icon={<ChevronLeft aria-hidden height={16} width={16} />}
          isDisabled={isBusy || !canStepTurnBack}
          label="Turn"
          tooltip="Previous turn"
          variant="secondary"
        />
        <Button
          clickAction={() => onStep(-1)}
          icon={<ChevronLeft aria-hidden height={16} width={16} />}
          isDisabled={isBusy || !canStepBack}
          isIconOnly
          label="Previous action"
          tooltip="Previous action"
          variant="secondary"
        />
      </ButtonGroup>

      {/* What is being read, in the game's own terms first: a player looking
          for a moment remembers the day and whose turn it was, not how many
          actions had been taken by then. */}
      <HStack align="center" gap={2} wrap="wrap">
        <Text type="label">
          Day {day}
          {turnHolder === null ? "" : ` · ${turnHolder}`}
        </Text>
        <Text type="supporting">
          {isAtLatest || position === null ? latestLabel : `${position.index} of ${position.total}`}
        </Text>
      </HStack>

      <ButtonGroup label="Step on" size="sm">
        <Button
          clickAction={() => onStep(1)}
          icon={<ChevronRight aria-hidden height={16} width={16} />}
          isDisabled={isBusy || !canStepForward}
          isIconOnly
          label="Next action"
          tooltip="Next action"
          variant="secondary"
        />
        <Button
          clickAction={() => onTurnStep(1)}
          icon={<ChevronRight aria-hidden height={16} width={16} />}
          isDisabled={isBusy || !canStepTurnForward}
          label="Turn"
          tooltip="Next turn"
          variant="secondary"
        />
        <Button
          clickAction={onSeekLatest}
          icon={<ArrowBarRight aria-hidden height={16} width={16} />}
          isDisabled={isBusy || isAtLatest}
          isIconOnly
          label={`Go to ${latestLabel.toLowerCase()}`}
          tooltip={latestLabel}
          variant="secondary"
        />
      </ButtonGroup>
    </HStack>
  );
}

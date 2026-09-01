/*
 * THE SEAT ROSTER
 *
 * THESIS: a match against the computer should be one screen and one press. The
 *   map board already knows how many seats the chosen battlefield has, so the
 *   seats belong here rather than behind a lobby the host would open only to
 *   fill it and close it again.
 * FORM: one row for each seat the map holds, each row a choice between leaving
 *   the seat open for somebody and handing it to the computer. The host's own
 *   seat is claimed in the lobby, as it always was, which is why at least one
 *   row has to stay open.
 */

import { List, ListItem } from "@astryxdesign/core/List";
import { Selector } from "@astryxdesign/core/Selector";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Heading } from "@astryxdesign/core/Heading";
import { Text } from "@astryxdesign/core/Text";
import { aiProfileDisplays, DEFAULT_AI_PROFILE_ID } from "../ai_profiles.ts";
import type { AiProfileId } from "../schemas.ts";

/** The value a row carries when nobody has been put in the seat. */
const OPEN = "open";

export interface SeatRosterProps {
  /** How many seats the map holds. */
  playerCount: number;
  /**
   * The opponent in each seat, by slot index. A slot with no entry is open.
   *
   * A map keeps the roster sparse, which is what a mostly open lobby is.
   */
  aiSeats: ReadonlyMap<number, AiProfileId>;
  onChange: (aiSeats: ReadonlyMap<number, AiProfileId>) => void;
}

const seatOptions = [
  { value: OPEN, label: "Open", description: "Anyone can take this seat." },
  ...aiProfileDisplays.map((profile) => ({
    value: profile.id,
    label: `${profile.label} CPU`,
    description: profile.blurb,
  })),
];

export function SeatRoster({ aiSeats, onChange, playerCount }: SeatRosterProps) {
  const openSeats = playerCount - aiSeats.size;

  function setSeat(slotIndex: number, value: string): void {
    const next = new Map(aiSeats);
    if (value === OPEN) {
      next.delete(slotIndex);
    } else {
      next.set(slotIndex, value as AiProfileId);
    }
    onChange(next);
  }

  return (
    <VStack gap={3}>
      <VStack gap={1}>
        <Heading level={3}>Seats</Heading>
        <Text color="secondary">
          {aiSeats.size === 0
            ? "Every seat is open. Hand one to the computer to play without waiting for anybody."
            : `${aiSeats.size} of ${playerCount} seats are the computer's. It takes its turn the moment the board reaches it.`}
        </Text>
      </VStack>

      <List hasDividers density="compact" header={null}>
        {Array.from({ length: playerCount }, (_unused, slotIndex) => {
          const seated = aiSeats.get(slotIndex);
          // The last open seat is the one the host claims in the lobby, so it
          // is held open rather than refused after the lobby fails to start.
          const isLastOpenSeat = seated === undefined && openSeats <= 1;

          return (
            <ListItem
              endContent={
                <Selector
                  disabledMessage="Keep one seat open for yourself."
                  isDisabled={isLastOpenSeat}
                  isLabelHidden
                  label={`Who holds seat ${slotIndex + 1}`}
                  onChange={(value) => setSeat(slotIndex, value)}
                  options={seatOptions}
                  size="sm"
                  value={seated ?? OPEN}
                  width={220}
                />
              }
              key={slotIndex}
              label={`Seat ${slotIndex + 1}`}
            />
          );
        })}
      </List>

      <HStack gap={2}>
        <Text color="secondary" type="label">
          You claim your own seat in the lobby, so one seat always stays open.
        </Text>
      </HStack>
    </VStack>
  );
}

/** A roster with nobody seated, which is what a new match starts as. */
export const NO_AI_SEATS: ReadonlyMap<number, AiProfileId> = new Map();

/** The opponent a seat takes the first time a host hands one over. */
export { DEFAULT_AI_PROFILE_ID };

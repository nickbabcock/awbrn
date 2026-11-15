/*
 * The seats of a new match in turn order: yours, wherever you sit, and every
 * other seat left open or handed to the computer.
 */

import { Selector } from "@astryxdesign/core/Selector";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import { SeatStamp } from "#/play/Opponents.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { aiProfileDisplays, DEFAULT_AI_PROFILE_ID } from "../ai_profiles.ts";
import type { AiProfileId } from "../schemas.ts";

/** The value a row carries when nobody has been put in the seat. */
const OPEN = "open";
/** The value that moves the host into a row. */
const YOU = "you";

export interface RosterSeats {
  /** The seat the host takes in the lobby. */
  hostSlot: number;
  /** The opponent in each seat, by slot index. A slot with no entry is open. */
  aiSeats: ReadonlyMap<number, AiProfileId>;
}

export interface SeatRosterProps extends RosterSeats {
  /** Each seat's army, in turn order; its length is the number of seats. */
  factionIds: readonly number[];
  onChange: (seats: RosterSeats) => void;
  onHostArmyChange: (factionId: number) => void;
  /** The host's name, on their seat. */
  viewerName: string | null;
}

const seatOptions = [
  { value: YOU, label: "You" },
  { value: OPEN, label: "Open" },
  ...aiProfileDisplays.map((profile) => ({
    value: profile.id,
    label: `${profile.label} CPU`,
  })),
];

export function SeatRoster({
  aiSeats,
  factionIds,
  hostSlot,
  onChange,
  onHostArmyChange,
  viewerName,
}: SeatRosterProps) {
  function setSeat(slotIndex: number, value: string): void {
    const next = new Map(aiSeats);
    if (value === YOU) {
      // The host and whoever held the seat trade places.
      const occupant = aiSeats.get(slotIndex);
      next.delete(slotIndex);
      if (occupant) next.set(hostSlot, occupant);
      onChange({ hostSlot: slotIndex, aiSeats: next });
      return;
    }
    if (value === OPEN) {
      next.delete(slotIndex);
    } else {
      next.set(slotIndex, value as AiProfileId);
    }
    onChange({ hostSlot, aiSeats: next });
  }

  return (
    <VStack aria-label="Seats, in turn order" role="group" xstyle={styles.list}>
      {factionIds.map((factionId, slotIndex) => {
        const factionCode = getFactionById(factionId)?.code ?? "os";
        return (
          <VStack key={slotIndex} xstyle={styles.row}>
            <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.number]}>
              {slotIndex + 1}
            </Text>
            <HStack as="span" xstyle={styles.crest}>
              {slotIndex === hostSlot ? (
                <FactionCrest factionCode={factionCode} onChange={onHostArmyChange} size={28} />
              ) : (
                <FactionLogo factionCode={factionCode} isLabelHidden size={28} />
              )}
            </HStack>
            {slotIndex === hostSlot ? (
              <HStack as="span" xstyle={styles.who}>
                <SeatStamp
                  coId={null}
                  factionCode={factionCode}
                  isYours
                  label="Your commander is picked in the lobby"
                />
                <b>{viewerName ?? "You"}</b>
                {/* Signed out, the seat's name is already "You". */}
                {viewerName !== null ? (
                  <HStack as="span" xstyle={styles.you}>
                    YOU
                  </HStack>
                ) : null}
              </HStack>
            ) : (
              <Selector
                isLabelHidden
                label={`Who holds seat ${slotIndex + 1}`}
                onChange={(value) => setSeat(slotIndex, value)}
                options={seatOptions}
                presentation="adaptive"
                value={aiSeats.get(slotIndex) ?? OPEN}
                width="100%"
              />
            )}
          </VStack>
        );
      })}
    </VStack>
  );
}

/** A roster with nobody seated, which is what a new match starts as. */
export const NO_AI_SEATS: ReadonlyMap<number, AiProfileId> = new Map();

/** The opponent a seat takes the first time a host hands one over. */
export { DEFAULT_AI_PROFILE_ID };

const styles = stylex.create({
  list: {
    display: "grid",
    gap: "8px",
  },
  row: {
    display: "grid",
    gridTemplateColumns: "16px auto minmax(0, 1fr)",
    alignItems: "center",
    gap: "12px",
    minBlockSize: "44px",
  },
  // One column for every row: a plain crest sits where the host's crest key
  // draws its own, inside the key's border and padding.
  crest: {
    inlineSize: "60px",
    paddingInlineStart: {
      default: "7px",
      ":has(button)": 0,
    },
  },
  number: {
    color: awbrnVars.textTertiary,
    fontSize: textSizeVars["--font-size-sm"],
  },
  who: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    minInlineSize: 0,
    color: colorVars["--color-text-primary"],
  },
  you: {
    display: "inline-flex",
    alignItems: "center",
    blockSize: "18px",
    paddingInline: "6px",
    borderRadius: "4px",
    backgroundColor: awbrnVars.ink100,
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.textMicro,
    fontWeight: 750,
    letterSpacing: "0.14em",
  },
});

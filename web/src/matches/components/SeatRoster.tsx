/*
 * THE SEAT ROSTER
 *
 * The seats of a new match in turn order: yours first, with your stamp, name
 * and YOU (your commander is picked in the lobby), and every other seat a choice
 * between leaving it open for somebody and handing it to the computer. A
 * match against the computer is one screen and one press.
 */

import { Selector } from "@astryxdesign/core/Selector";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { SeatStamp } from "#/play/Opponents.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { aiProfileDisplays, DEFAULT_AI_PROFILE_ID } from "../ai_profiles.ts";
import type { AiProfileId } from "../schemas.ts";

/** The value a row carries when nobody has been put in the seat. */
const OPEN = "open";

/** The seat the host takes in the lobby: the first in turn order. */
const HOST_SLOT = 0;

export interface SeatRosterProps {
  /** Each seat's army, in turn order; its length is the number of seats. */
  factionCodes: readonly string[];
  /** The opponent in each seat, by slot index. A slot with no entry is open. */
  aiSeats: ReadonlyMap<number, AiProfileId>;
  onChange: (aiSeats: ReadonlyMap<number, AiProfileId>) => void;
  /** The host's name, on the first seat. */
  viewerName: string | null;
}

const seatOptions = [
  { value: OPEN, label: "Open" },
  ...aiProfileDisplays.map((profile) => ({
    value: profile.id,
    label: `${profile.label} CPU`,
  })),
];

export function SeatRoster({ aiSeats, factionCodes, onChange, viewerName }: SeatRosterProps) {
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
    <VStack aria-label="Seats, in turn order" role="group" xstyle={styles.list}>
      {factionCodes.map((factionCode, slotIndex) => (
        <VStack key={slotIndex} xstyle={styles.row}>
          <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.number]}>
            {slotIndex + 1}
          </Text>
          <FactionLogo factionCode={factionCode} isFramed={false} isLabelHidden size={28} />
          {slotIndex === HOST_SLOT ? (
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
      ))}
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

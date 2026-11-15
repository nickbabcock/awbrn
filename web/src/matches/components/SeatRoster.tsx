/*
 * The seats of a new match in turn order: yours, wherever you sit, and every
 * other seat left open or handed to the computer.
 */

import { Icon } from "@astryxdesign/core/Icon";
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
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconArrowDown, IconArrowUp } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { aiProfileDisplays, DEFAULT_AI_PROFILE_ID } from "../ai_profiles.ts";
import type { AiProfileId } from "../schemas.ts";
import { moveSeat } from "../teams.ts";
import { TeamKey } from "./Teams.tsx";

/** The value a row carries when nobody has been put in the seat. */
const OPEN = "open";

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
  /** Each seat's side, by slot index; null in a free-for-all. */
  teams: readonly number[] | null;
  onTeamChange: (teams: number[]) => void;
  /** The host's name, on their seat. */
  viewerName: string | null;
}

const seatOptions = [
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
  onTeamChange,
  teams,
  viewerName,
}: SeatRosterProps) {
  const factionCodes = factionIds.map((factionId) => getFactionById(factionId)?.code ?? "os");
  function setSeat(slotIndex: number, value: string): void {
    const next = new Map(aiSeats);
    if (value === OPEN) {
      next.delete(slotIndex);
    } else {
      next.set(slotIndex, value as AiProfileId);
    }
    onChange({ hostSlot, aiSeats: next });
  }

  // The host and whoever held the seat trade places; armies stay with seats.
  function moveHost(slotIndex: number): void {
    const next = new Map(aiSeats);
    const occupant = aiSeats.get(slotIndex);
    next.delete(slotIndex);
    if (occupant) next.set(hostSlot, occupant);
    onChange({ hostSlot: slotIndex, aiSeats: next });
  }

  return (
    <VStack aria-label="Seats, in turn order" role="group" xstyle={styles.list}>
      {factionIds.map((_, slotIndex) => {
        const factionCode = factionCodes[slotIndex]!;
        return (
          <VStack key={slotIndex} xstyle={[styles.row, teams && styles.rowTeams]}>
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
                {/* On a phone with team keys the crest key already shows the
                    army, so the empty stamp gives its room to the move keys. */}
                <HStack as="span" xstyle={teams && styles.phoneHidden}>
                  <SeatStamp
                    coId={null}
                    factionCode={factionCode}
                    isYours
                    label="Your commander is picked in the lobby"
                  />
                </HStack>
                <b>{viewerName ?? "You"}</b>
                {/* Signed out, the seat's name is already "You". */}
                {viewerName !== null ? (
                  <HStack as="span" xstyle={styles.you}>
                    YOU
                  </HStack>
                ) : null}
                {/* Turn order is the seat: these move the host through it. */}
                <HStack as="span" xstyle={styles.move}>
                  <Button
                    icon={<Icon icon={IconArrowUp} />}
                    isDisabled={slotIndex === 0}
                    isIconOnly
                    label={`Move to seat ${slotIndex}`}
                    onClick={() => moveHost(slotIndex - 1)}
                    size="sm"
                    tooltip="Move earlier"
                    variant="ghost"
                  />
                  <Button
                    icon={<Icon icon={IconArrowDown} />}
                    isDisabled={slotIndex === factionIds.length - 1}
                    isIconOnly
                    label={`Move to seat ${slotIndex + 2}`}
                    onClick={() => moveHost(slotIndex + 1)}
                    size="sm"
                    tooltip="Move later"
                    variant="ghost"
                  />
                </HStack>
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
            {teams ? (
              <TeamKey
                factionCodes={factionCodes}
                onChange={(team) => {
                  const next = moveSeat(teams, slotIndex, team);
                  if (next) onTeamChange(next);
                }}
                slotIndex={slotIndex}
                teams={teams}
                viewerTeam={teams[hostSlot] ?? null}
              />
            ) : null}
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
  rowTeams: {
    gridTemplateColumns: "16px auto minmax(0, 1fr) auto",
  },
  phoneHidden: {
    display: {
      default: "inline-flex",
      [pageLayout.phoneMedia]: "none",
    },
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
  move: {
    display: "inline-flex",
    marginInlineStart: "auto",
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

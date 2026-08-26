/**
 * THE CO INTEL BOARD
 *
 * Every commanding officer at once, one tile each, at the same size. A CO is
 * recognized by a face before a name, so the board is portraits and the name
 * is the readout under them, and the tiles do not change shape between the
 * screen that bans a CO and the screen that picks one.
 *
 * A banned CO is not hidden. It keeps its tile and wears the strike, because a
 * player looking for Grit has to find out that Grit is gone rather than fail
 * to find him.
 */

import { Grid } from "@astryxdesign/core/Grid";
import { SelectableCard } from "@astryxdesign/core/SelectableCard";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { borderVars, colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useMemo } from "react";
import { coRoster, getCoById, type CoRosterEntry } from "#/co_roster.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { loadCoPortraitCatalog } from "#/components/co_portraits.ts";

/**
 * How wide a tile runs and how many screen pixels its portrait takes.
 *
 * The board is drawn at two sizes because it is read in two places: a briefing
 * panel that has the whole page to spend, and a seat card in one column of the
 * lobby. Both draw the portrait at a whole multiple of its 32 native pixels.
 */
const BOARD_SIZES = {
  md: { portrait: 64, tile: 84 },
  sm: { portrait: 32, tile: 56 },
} as const;

/** How many screen pixels a portrait takes in the banned strip. */
const STRIP_PORTRAIT_SIZE = 32;

type CoBoardProps = {
  /** The COs this match has taken away. */
  bannedCoIds: ReadonlySet<number>;
  /** Locks every tile, for a board that is being saved or is read-only. */
  isDisabled?: boolean;
  /** How much room the board has. Defaults to the full-width briefing size. */
  size?: keyof typeof BOARD_SIZES;
} & (
  | {
      /** Pressing a tile bans that CO, or gives it back. */
      mode: "ban";
      onToggleBan: (coId: number) => void;
    }
  | {
      /** Pressing a tile makes that CO the seat's. Banned tiles refuse. */
      mode: "pick";
      selectedCoId: number | null;
      onPick: (coId: number) => void;
    }
);

export function CoBoard(props: CoBoardProps) {
  const { bannedCoIds, isDisabled = false, mode, size = "md" } = props;
  const catalog = useMemo(() => loadCoPortraitCatalog(), []);
  const { portrait, tile } = BOARD_SIZES[size];

  return (
    <Grid columns={{ minWidth: tile, max: 10, repeat: "fill" }} gap={size === "sm" ? 1.5 : 2}>
      {coRoster.map((co) => {
        const isBanned = bannedCoIds.has(co.awbwId);
        const isSelected = mode === "ban" ? isBanned : props.selectedCoId === co.awbwId;

        return (
          <SelectableCard
            isDisabled={isDisabled || (mode === "pick" && isBanned)}
            isSelected={isSelected}
            key={co.awbwId}
            label={tileLabel(co, { isBanned, mode })}
            onChange={() =>
              mode === "ban" ? props.onToggleBan(co.awbwId) : props.onPick(co.awbwId)
            }
            padding={1}
          >
            <VStack align="center" gap={1}>
              <Section
                padding={0}
                variant="muted"
                xstyle={[styles.well, isBanned && styles.wellBanned]}
              >
                <CoPortrait
                  catalog={catalog}
                  coKey={co.key}
                  fallbackLabel={co.displayName}
                  hasFrame={false}
                  size={portrait}
                />
              </Section>
              <Text hasStrikethrough={isBanned} justify="center" maxLines={1} type="label">
                {co.displayName}
              </Text>
            </VStack>
          </SelectableCard>
        );
      })}
    </Grid>
  );
}

/**
 * The COs this match took away, named and struck.
 *
 * A player reads this before claiming a seat, so it lists only what is gone
 * rather than making them scan the whole roster for the strikes.
 */
export function BannedCoList({ bannedCoIds }: { bannedCoIds: readonly number[] }) {
  const catalog = useMemo(() => loadCoPortraitCatalog(), []);
  const banned = bannedCoIds
    .map((coId) => getCoById(coId))
    .filter((co): co is CoRosterEntry => co !== null);

  if (banned.length === 0) {
    return (
      <Text color="secondary" type="supporting">
        Every CO is available in this match.
      </Text>
    );
  }

  return (
    <HStack as="ul" gap={3} wrap="wrap" xstyle={styles.plainList}>
      {banned.map((co) => (
        <HStack align="center" as="li" gap={1.5} key={co.awbwId}>
          <Section
            padding={0}
            variant="muted"
            xstyle={[styles.well, styles.stripWell, styles.wellBanned]}
          >
            <CoPortrait
              catalog={catalog}
              coKey={co.key}
              fallbackLabel={co.displayName}
              hasFrame={false}
              size={STRIP_PORTRAIT_SIZE}
            />
          </Section>
          <Text hasStrikethrough type="label">
            {co.displayName}
          </Text>
        </HStack>
      ))}
    </HStack>
  );
}

function tileLabel(
  co: CoRosterEntry,
  { isBanned, mode }: { isBanned: boolean; mode: "ban" | "pick" },
): string {
  if (mode === "ban") return isBanned ? `${co.displayName}, banned` : `Ban ${co.displayName}`;
  return isBanned ? `${co.displayName}, banned in this match` : `Choose ${co.displayName}`;
}

const styles = stylex.create({
  // The portrait already sits inside the tile's outline, so it takes the
  // recessed fill and none of the border.
  well: {
    borderRadius: "var(--radius-element)",
    lineHeight: 0,
    overflow: "hidden",
    position: "relative",
  },
  stripWell: {
    flex: "0 0 auto",
  },
  // A banned CO is washed out and struck corner to corner. Both marks are
  // drawn on the well rather than over the whole tile, so they land on the
  // face and leave the name under it at full strength. The wash goes under
  // the strike rather than over the portrait as one opacity, which would take
  // the strike down with it.
  wellBanned: {
    "::before": {
      backgroundColor: colorVars["--color-background-muted"],
      content: "''",
      inset: 0,
      opacity: 0.55,
      position: "absolute",
    },
    "::after": {
      backgroundImage: `linear-gradient(to bottom right, transparent calc(50% - ${borderVars["--border-width"]}), ${colorVars["--color-error"]} calc(50% - ${borderVars["--border-width"]}), ${colorVars["--color-error"]} calc(50% + ${borderVars["--border-width"]}), transparent calc(50% + ${borderVars["--border-width"]}))`,
      content: "''",
      inset: 0,
      position: "absolute",
    },
  },
  plainList: {
    listStyleType: "none",
    margin: 0,
    padding: 0,
  },
});

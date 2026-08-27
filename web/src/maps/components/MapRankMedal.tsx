/**
 * The grade the site gives a map revision.
 *
 * Advance Wars ends a battle by stamping one letter on the report, and that
 * is what a rank is here, so it is drawn as that plate rather than as a chip
 * in a row of metadata: cream, outlined in the one black, the letter in the
 * signage voice, and a bar of the rank's color inset across the top. The bar
 * is the gesture a faction panel already uses, so a rank reads as an identity
 * the map holds and not as a severity.
 *
 * A revision with no rank is not a fifth grade. It is a slot nothing has been
 * put in yet, so it takes the dashed empty well this system already gives a
 * place waiting to be filled.
 */

import { Text } from "@astryxdesign/core/Text";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import { VStack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import type { MapRank } from "#/maps/schemas.ts";

export type MapRankMedalSize = "sm" | "md" | "lg";

/** What each grade means, said where a moderator has to choose one. */
export const MAP_RANK_DESCRIPTIONS: Record<MapRank, string> = {
  S: "Plays well enough to put in front of anybody.",
  A: "A good map with nothing wrong with it.",
  B: "Playable, with a flaw worth knowing about.",
  C: "Held for the record rather than recommended.",
};

export function MapRankMedal({
  rank,
  size = "md",
}: {
  rank: MapRank | null;
  size?: MapRankMedalSize;
}) {
  if (rank === null) {
    return (
      <VStack
        align="center"
        aria-label="Unranked"
        justify="center"
        role="img"
        xstyle={[styles.medal, styles.unranked, sizes[size]]}
      >
        <Text aria-hidden type="label" xstyle={[styles.dash, letterSizes[size]]}>
          &ndash;
        </Text>
      </VStack>
    );
  }

  return (
    <VStack
      align="center"
      aria-label={`Rank ${rank}`}
      justify="center"
      role="img"
      xstyle={[styles.medal, styles.ranked, bars[rank], sizes[size]]}
    >
      <Text aria-hidden type="display-2" xstyle={[styles.letter, letterSizes[size]]}>
        {rank}
      </Text>
    </VStack>
  );
}

const styles = stylex.create({
  medal: {
    aspectRatio: "1",
    borderRadius: "var(--radius-element)",
    borderStyle: "solid",
    borderWidth: "var(--border-width)",
    flexShrink: 0,
  },
  ranked: {
    backgroundColor: colorVars["--color-background-surface"],
    borderColor: colorVars["--color-border-emphasized"],
  },
  // The slot a rank has not been put in: recessed, dashed, and empty.
  unranked: {
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: awbrnVars.colorBorderDisabled,
    borderStyle: "dashed",
  },
  letter: {
    color: colorVars["--color-text-primary"],
    lineHeight: 1,
    // The bar takes the top of the plate, so the letter sits under it.
    paddingBlockStart: "var(--spacing-1)",
  },
  dash: {
    color: colorVars["--color-text-disabled"],
    lineHeight: 1,
  },
});

/** How deep the rank's color sits across the top of the plate. */
const BAR_SIZE = "5px";

/**
 * The bar of each grade, from the color families the theme tunes to the
 * armies. S takes the command orange, because the top grade is the one thing
 * on a board of maps worth acting on. The hard step and its soft landing ride
 * with it, the same pair every panel in this system casts.
 */
const bars = stylex.create({
  S: {
    boxShadow: `inset 0 ${BAR_SIZE} 0 0 ${colorVars["--color-accent"]}, var(--shadow-low)`,
  },
  A: {
    boxShadow: `inset 0 ${BAR_SIZE} 0 0 ${colorVars["--color-border-green"]}, var(--shadow-low)`,
  },
  B: {
    boxShadow: `inset 0 ${BAR_SIZE} 0 0 ${colorVars["--color-border-blue"]}, var(--shadow-low)`,
  },
  C: {
    boxShadow: `inset 0 ${BAR_SIZE} 0 0 ${colorVars["--color-border-gray"]}, var(--shadow-low)`,
  },
});

const sizes = stylex.create({
  sm: { width: "28px" },
  md: { width: "44px" },
  lg: { width: "76px" },
});

const letterSizes = stylex.create({
  sm: { fontSize: "var(--font-size-sm)" },
  md: { fontSize: "var(--font-size-lg)" },
  lg: { fontSize: "var(--font-size-3xl)" },
});

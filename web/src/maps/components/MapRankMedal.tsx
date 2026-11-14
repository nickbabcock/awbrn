/**
 * The grade the site gives a map revision: a pennant with one letter. The
 * letter is the medal's accessible name, so it never rides on the color alone.
 */

import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { typeScaleVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import type { MapRank } from "#/maps/schemas.ts";

export function MapRankMedal({
  isUnrankedShown = false,
  rank,
  size,
  xstyle,
}: {
  isUnrankedShown?: boolean;
  rank: MapRank | null;
  size: "xs" | "sm" | "lg";
  xstyle?: stylex.StyleXStyles;
}) {
  if (rank === null && !isUnrankedShown) return null;

  return (
    <Stack
      aria-label={rank === null ? "Unranked" : `Rank ${rank}`}
      as="span"
      role="img"
      xstyle={[styles.medal, sizes[size], rank === null ? styles.none : tones[rank], xstyle]}
    >
      <Text aria-hidden="true" color="inherit" type="inherit">
        {rank ?? "–"}
      </Text>
    </Stack>
  );
}

const styles = stylex.create({
  medal: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    clipPath: "polygon(0 0, 100% 0, 100% 78%, 50% 100%, 0 78%)",
    color: awbrnVars.textOnInkStrong,
    fontFamily: awbrnVars.fontUi,
    fontStretch: "62%",
    fontWeight: 850,
    lineHeight: 1,
    wordSpacing: "normal",
  },
  none: {
    backgroundColor: awbrnVars.paperWell,
    color: awbrnVars.textTertiary,
  },
});

const tones = stylex.create({
  S: { backgroundColor: awbrnVars.gold, color: awbrnVars.ink950 },
  A: { backgroundColor: awbrnVars.ink900 },
  B: { backgroundColor: awbrnVars.blue },
  // C is slate: the tertiary ink, which holds white at better than 4.5:1.
  C: { backgroundColor: awbrnVars.textTertiary },
});

const sizes = stylex.create({
  xs: {
    inlineSize: "22px",
    blockSize: "26px",
    paddingBlockEnd: "4px",
    fontSize: typeScaleVars["--text-label-size"],
  },
  sm: {
    inlineSize: { default: "32px", [pageLayout.phoneMedia]: "26px" },
    blockSize: { default: "42px", [pageLayout.phoneMedia]: "34px" },
    paddingBlockEnd: { default: "7px", [pageLayout.phoneMedia]: "6px" },
    fontSize: { default: "18px", [pageLayout.phoneMedia]: typeScaleVars["--text-body-size"] },
  },
  lg: {
    inlineSize: "56px",
    blockSize: "64px",
    paddingBlockEnd: "9px",
    fontSize: "32px",
  },
});

import { Stack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/**
 * The band marks what waits on the viewer, never decoration. `isOverlay` pins
 * it along the top edge of a positioned parent that cannot give it a row.
 */
export function AirmailBand({
  isOverlay = false,
  thickness = 6,
  xstyle,
}: {
  isOverlay?: boolean;
  thickness?: 4 | 5 | 6;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <Stack
      aria-hidden="true"
      as="span"
      xstyle={[styles.band, styles.thickness(thickness), isOverlay && styles.overlay, xstyle]}
    />
  );
}

const styles = stylex.create({
  band: {
    display: "block",
    flexShrink: 0,
    inlineSize: "100%",
    backgroundImage: awbrnVars.band,
    backgroundSize: "42px 6px",
  },
  thickness: (px: number) => ({
    blockSize: `${px}px`,
  }),
  overlay: {
    position: "absolute",
    insetInline: 0,
    insetBlockStart: 0,
    zIndex: 1,
  },
});

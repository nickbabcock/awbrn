import { Stack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/**
 * The airmail band: repeating red, white, blue, white parallelograms along the
 * top edge of what waits on the viewer.
 *
 * **Band Means Owed.** Only on Your move, the turn strip and your army on your
 * turn, and a pairing to confirm. Never as decoration.
 *
 * - In flow (default): a block at the top of its parent, e.g. the first child
 *   of a `Sheet` (which `Sheet isOwed` already does for you).
 * - `isOverlay`: absolutely placed along the top edge of a positioned parent,
 *   for a plate, a ledger row or a dock that cannot give it a row.
 *
 *   <AirmailBand thickness={4} isOverlay />
 */
export function AirmailBand({
  isOverlay = false,
  thickness = 6,
  xstyle,
}: {
  isOverlay?: boolean;
  /** 4px on plates and rows, 5px on docks and strips, 6px on a sheet. */
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

/**
 * What every board of maps shares.
 *
 * Two screens deal the same board: the catalog, where a plate opens a map,
 * and the create screen, where a plate is chosen. They are one board and are
 * held to one column rule and one readout, so a map does not change size or
 * change what it says about itself depending on why it is being looked at.
 */

/**
 * The grid every board of plates is dealt onto.
 *
 * The column count is high for the width because a plate's well cannot grow
 * with it: map art is drawn at a whole multiple of its own pixels, so a well
 * wider than the largest picture at that multiple is tan nothing. Narrow
 * columns keep the art against the edges of its well and fit more of the
 * catalog in one look.
 */
export const MAP_BOARD_COLUMNS = { minWidth: 156, max: 7, repeat: "fill" } as const;

/** Plates drawn while the first page of a board is still on its way. */
export const MAP_BOARD_LOADING_PLATES = 8;

/**
 * What a board holds, in the HUD voice.
 *
 * A narrowed board says what it found; a whole one says what AWBRN holds,
 * because those are two different facts and the count alone does not say
 * which one is on screen.
 */
export function mapBoardSummary({
  count,
  hasMore,
  isNarrowed,
  isPending,
}: {
  count: number;
  hasMore: boolean;
  isNarrowed: boolean;
  isPending: boolean;
}): string {
  if (isPending) return "Reading the catalog";
  const plural = count === 1 && !hasMore ? "" : "s";
  return `${count}${hasMore ? "+" : ""} map${plural} ${isNarrowed ? "found" : "held"}`;
}

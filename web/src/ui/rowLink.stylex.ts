import * as stylex from "@stylexjs/stylex";

/**
 * Marks the one link a row stretches over itself, so the row can wear that
 * link's focus ring while the other links inside it (seat names) keep their
 * own.
 */
export const rowLinkMarker = stylex.defineMarker();

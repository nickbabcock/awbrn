import * as stylex from "@stylexjs/stylex";

/**
 * The match screen's geometry: the frame DESIGN.md measures (the rail, the
 * strip, the foot), the key and row sizes the board's controls share, and the
 * small marks the armies draw. Spacing, radii and colors come from the theme
 * (`spacingVars`, `radiusVars`, `awbrnVars`); these are the sizes it has no
 * name for.
 */
export const matchLayout = stylex.defineConsts({
  /** The rail beside the stage on desktop. */
  railWidth: "360px",
  /** The desktop turn strip over the table, and the review foot under it. */
  stripHeight: "80px",
  footHeight: "56px",
  screenMinHeight: "520px",
  screenMinHeightPhone: "480px",
  /** A key or a line a finger can hit: icon keys, board keys, the tile line. */
  target: "44px",
  /** The phone dock's keys: 64 wide, 52 tall. */
  dockKeyWidth: "64px",
  dockKeyHeight: "52px",
  /** The glyph on a 52px phone key: the dock's and the review bar's. */
  dockGlyph: "22px",
  /** A ledger row and a turn-order cell. */
  rowHeight: "64px",
  /** The narrowest a turn-order cell gets before the row scrolls. */
  cellMinWidth: "56px",
  /** The power key in the rail, and in a ledger row. */
  railPowerKey: "34px",
  rowPowerKey: "26px",
  /** The scale between the zoom keys (`2×`). */
  scaleReadout: "36px",
  /** The ledger's turn number column. */
  ledgerNumber: "14px",
  /** The board's fade into the table on a cropped side. */
  fadeDepth: "16px",
  /** The postmark's cancel lines beside it in the strip. */
  cancelWidth: "46px",
  cancelHeight: "30px",
  /** The Watching tag. */
  tagHeight: "22px",
  /** YOU beside a name. */
  youHeight: "18px",
  youHeightSmall: "17px",
  /** A connection dot, and the green mover dot. */
  dot: "8px",
  movingDot: "7px",
  /** The green foot under the mover's turn-order cell. */
  footWidth: "24px",
  moverFootHeight: "3px",
  /** Meter stars: small buy the power, big the super power. */
  star: "13px",
  starSmall: "9px",
  starBig: "19px",
  starBigSmall: "12px",
  /** Board keys floating at the board's edge: ink 900 at 94%. */
  inkFloat: "color-mix(in srgb, var(--color-ink-900) 94%, transparent)",
  /** The board's edge fading into the table: 55% ink 950. */
  boardFade: "color-mix(in srgb, var(--color-ink-950) 55%, transparent)",
  /** The table's glow under the board. */
  tableGlow: "color-mix(in srgb, var(--color-ink-700) 35%, transparent)",
  /** The map outside the locator's view, dimmed. */
  locatorDim: "color-mix(in srgb, var(--color-ink-950) 45%, transparent)",
});

/**
 * The match screen's type sizes. DESIGN.md's label (12px) and figure (12.5px)
 * hold most of it; the rest are the strip and line titles and the steps
 * between them that a dense rail needs.
 */
export const matchType = stylex.defineConsts({
  /** YOU and OUT on a turn-order stamp. */
  tag: "10.5px",
  /** YOU beside a name; the view key's word. */
  micro: "11px",
  /** A ledger row's turn number. */
  ordinal: "11.5px",
  label: "12px",
  figure: "12.5px",
  note: "13px",
  /** Clock chips, stat values, the strip's second line. */
  detail: "13.5px",
  small: "14px",
  /** An army's name in a row or a plate. */
  name: "14.5px",
  body: "15px",
  /** An army's name on its card. */
  cardName: "16.5px",
  /** The mover's clock in the strip. */
  clock: "20px",
  /** The phone turn line's title, and the desktop strip's. */
  lineTitle: "21px",
  stripTitle: "26px",
});

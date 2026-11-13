import * as stylex from "@stylexjs/stylex";

/**
 * The board's missile colors, which the strike list's swatches copy.
 *
 * They are board art, not palette: the engine paints each missile's outline
 * and badge in these (`strike_color` in
 * `crates/awbrn-client/src/modes/planner/mod.rs`), and no army, terrain or
 * inspection field uses them. Change both together.
 */
export const missileVars = stylex.defineConsts({
  /** The infantry missile, `srgb(1.0, 0.88, 0.30)`. */
  infantry: "#ffe14d",
  /** The funds missile, `srgb(1.0, 0.18, 0.82)`. */
  funds: "#ff2fd1",
});

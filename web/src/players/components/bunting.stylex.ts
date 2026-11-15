import * as stylex from "@stylexjs/stylex";

/**
 * The bunting's drawn sizes: the design's fixed pixels, as the page frame's
 * widths are in `pageLayout`. Flag widths stay numbers in `Bunting.tsx`,
 * since it scales each flag's length and numeral from them.
 */
export const buntingSizes = stylex.defineConsts({
  headWidth: "136px",
  plotHeight: "116px",
  plotHeightPhone: "74px",
  unratedHeight: "60px",
  unratedHeightPhone: "52px",
  /** An unrated row's plot: 1.5 seasons at the 240px a season may take. */
  unratedWidth: "360px",
  /** Room at the plot's ends for half a flag and the open season's batten overhang. */
  plotInset: "17px",
  plotInsetPhone: "13px",
  /** The start pin, the no-games hook and the peak's caret. */
  mark: "9px",
  /** The hook's stem and the open season's batten. */
  batten: "4px",
  /** How far the batten reaches past the flag's hoist on each side. */
  battenOverhang: "3px",
  /** The string, the hook's ring and stem, and a short flag's hairline. */
  stroke: "1.5px",
  /** The peak's caret is a mark wide and this tall; its figure sits above it. */
  caretHeight: "6px",
  peakFigureLift: "13px",
  /** The season chip floats this far above its mark. */
  chipLift: "10px",
  /** Near either end of the plot, the chip's outer edge sits this far past its mark. */
  chipEdge: "16px",
  /** Headroom a scrolling plot keeps above its first row, so a season chip is not clipped. */
  chipRoom: "40px",
  axisHeight: "14px",
  /** The flag's head and the batten sit this far above the string, so it runs under the hoist. */
  flagLift: "1px",
  /** How far a flag's numeral may spill past its sides, so three digits still fit. */
  numeralBleed: "4px",
  /** The selected mark's ring clears the flag by this much. */
  selectedOffset: "3px",
  /** The gap between the peak's caret and its point on the string. */
  caretGap: "4px",
  /** The open season's wash stops this short of the plot's top and bottom. */
  washInset: "2px",
  /** Between the chip's season and its figures. */
  chipGap: "3px",
});

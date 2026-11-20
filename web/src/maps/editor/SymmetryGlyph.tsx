/**
 * The fold, drawn.
 *
 * A symmetry mode is a shape and not a word: "quarter turn" and "quarters"
 * are two sentences a map maker has to stop and parse, and two diagrams they
 * recognise. Each glyph shows the same three things — the board, the part of
 * it that is drawn on, and what happens to the rest — so the eight of them can
 * be read as one row rather than eight labels.
 *
 * Everything is stroked in `currentColor`, so a glyph on the pressed key wears
 * the key's own ink rather than carrying a second palette.
 */

import * as stylex from "@stylexjs/stylex";
import type { Symmetry } from "#/wasm/awbrn_wasm.js";

type Region =
  | { shape: "rect"; x: number; y: number; width: number; height: number }
  | { shape: "polygon"; points: string };

/** The part of the board a stroke is drawn on, for each fold. */
const REGIONS: Partial<Record<Symmetry, Region>> = {
  "mirror-left-right": { shape: "rect", x: 3, y: 3, width: 9, height: 18 },
  "mirror-top-bottom": { shape: "rect", x: 3, y: 3, width: 18, height: 9 },
  rotate180: { shape: "rect", x: 3, y: 3, width: 9, height: 18 },
  rotate90: { shape: "rect", x: 3, y: 3, width: 9, height: 9 },
  "quad-mirror": { shape: "rect", x: 3, y: 3, width: 9, height: 9 },
  "mirror-diagonal": { shape: "polygon", points: "3,3 21,3 21,21" },
  "mirror-anti-diagonal": { shape: "polygon", points: "3,3 21,3 3,21" },
};

/** The axis a mirror folds on. A turn has none: it has a pivot. */
const FOLDS: Partial<Record<Symmetry, string[]>> = {
  "mirror-left-right": ["M 12 2 L 12 22"],
  "mirror-top-bottom": ["M 2 12 L 22 12"],
  "quad-mirror": ["M 12 2 L 12 22", "M 2 12 L 22 12"],
  "mirror-diagonal": ["M 2 2 L 22 22"],
  "mirror-anti-diagonal": ["M 22 2 L 2 22"],
};

/** The turn, as the arc it sweeps and the head it ends on. */
const TURNS: Partial<Record<Symmetry, { arc: string; head: string }>> = {
  rotate180: { arc: "M 12 6 A 6 6 0 0 1 12 18", head: "10.5 18 14 16.2 14 19.8" },
  rotate90: { arc: "M 12 6 A 6 6 0 0 1 18 12", head: "18 13.5 16.2 10 19.8 10" },
};

export function SymmetryGlyph({ mode, size = 24 }: { mode: Symmetry; size?: number }) {
  const region = REGIONS[mode];
  const folds = FOLDS[mode];
  const turn = TURNS[mode];

  return (
    <svg
      aria-hidden="true"
      focusable="false"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      {...stylex.props(styles.glyph)}
    >
      {region?.shape === "rect" ? (
        <rect
          fill="currentColor"
          fillOpacity={0.2}
          height={region.height}
          width={region.width}
          x={region.x}
          y={region.y}
        />
      ) : null}
      {region?.shape === "polygon" ? (
        <polygon fill="currentColor" fillOpacity={0.2} points={region.points} />
      ) : null}

      <rect fill="none" height={20} stroke="currentColor" strokeWidth={2} width={20} x={2} y={2} />

      {folds?.map((fold) => (
        <path
          d={fold}
          fill="none"
          key={fold}
          stroke="currentColor"
          strokeDasharray="3 2.5"
          strokeWidth={1.5}
        />
      ))}

      {turn ? (
        <>
          <path d={turn.arc} fill="none" stroke="currentColor" strokeWidth={1.5} />
          <polygon fill="currentColor" points={turn.head} />
          <circle cx={12} cy={12} fill="currentColor" r={1.5} />
        </>
      ) : null}
    </svg>
  );
}

const styles = stylex.create({
  glyph: {
    display: "block",
    // The art is drawn on a whole-pixel grid; letting it inherit a text
    // baseline would push it half a pixel down inside its key.
    verticalAlign: "top",
  },
});

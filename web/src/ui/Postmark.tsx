import * as stylex from "@stylexjs/stylex";
import { useId } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * The postmark: a day is a postmark. Two rings, the date as ring text, a word
 * and a figure in the middle, struck at −9deg in stamp violet.
 *
 * - Full (default, 64px): the turn strip's day (`word="DAY"`, `figure={9}`,
 *   `ringText="NOV 12 · 2026"`), a match report's finish date
 *   (`word="NOV"`, `figure={10}`, `ringText="2026"`), a Record entry.
 * - `isCompact` (44px on a phone): the rings and the figure alone. At that size
 *   the ring text and the word fall below legibility; the accessible name
 *   still says it all.
 * - `tone="ink"` on the ink table and navy bars.
 * - `isNew`: the postmark lands in two frames (120ms, stepped: 1.25x, then
 *   1x) when End turn stamps a new day. Nothing moves under
 *   `prefers-reduced-motion`.
 *
 * `label` is the accessible name ("Day 9", "Finished Nov 10").
 *
 *   <Postmark figure={9} label="Day 9" ringText="NOV 12 · 2026" word="DAY" />
 */
export function Postmark({
  figure,
  isCompact = false,
  isNew = false,
  label,
  ringText,
  size,
  tone = "paper",
  word,
  xstyle,
}: {
  figure: number | string;
  isCompact?: boolean;
  isNew?: boolean;
  label: string;
  /** Repeated twice around the ring; keep it short and uppercase (`NOV 12 · 2026`). */
  ringText?: string;
  /** Rendered size in px; 64 by default, 44 when compact. */
  size?: number;
  tone?: "paper" | "ink";
  word?: string;
  xstyle?: stylex.StyleXStyles;
}) {
  const ringId = `postmark-ring-${useId().replace(/:/g, "")}`;
  const px = size ?? (isCompact ? 44 : 64);

  return (
    <svg
      aria-label={label}
      role="img"
      viewBox="0 0 100 100"
      {...stylex.props(
        styles.postmark,
        styles.size(px),
        tone === "ink" && styles.onInk,
        isNew && styles.isNew,
        xstyle,
      )}
    >
      <defs>
        <path d="M50,50 m-37,0 a37,37 0 1,1 74,0 a37,37 0 1,1 -74,0" id={ringId} />
      </defs>
      <circle cx="50" cy="50" r="47" strokeWidth="2.6" {...stylex.props(styles.ring)} />
      <circle cx="50" cy="50" r="28.5" strokeWidth="1.6" {...stylex.props(styles.ring)} />
      {!isCompact && ringText ? (
        <text {...stylex.props(styles.ringText)}>
          <textPath href={`#${ringId}`} startOffset="0">
            {`${ringText} · ${ringText} · `}
          </textPath>
        </text>
      ) : null}
      {!isCompact && word ? (
        <text x="50" y="46" {...stylex.props(styles.word)}>
          {word}
        </text>
      ) : null}
      <text
        x="50"
        y={isCompact || !word ? "61" : "72"}
        {...stylex.props(styles.figure, (isCompact || !word) && styles.figureAlone)}
      >
        {figure}
      </text>
    </svg>
  );
}

// In full ink from the first frame. The step jumps halfway at once, so the
// first frame shows the mark at 1.25x and the second strikes it at 1x.
const stampIn = stylex.keyframes({
  from: { transform: "rotate(-9deg) scale(1.5)" },
  to: { transform: "rotate(-9deg) scale(1)" },
});

const styles = stylex.create({
  postmark: {
    flexShrink: 0,
    color: awbrnVars.violet,
    transform: "rotate(-9deg)",
    overflow: "visible",
  },
  size: (px: number) => ({
    inlineSize: `${px}px`,
    blockSize: `${px}px`,
  }),
  onInk: {
    color: awbrnVars.violetOnInk,
  },
  isNew: {
    animationName: {
      default: stampIn,
      [pageLayout.reducedMotionMedia]: "none",
    },
    animationDuration: "120ms",
    animationTimingFunction: awbrnVars.easeStep,
    animationFillMode: "both",
  },
  ring: {
    fill: "none",
    stroke: "currentColor",
  },
  ringText: {
    fill: "currentColor",
    fontFamily: awbrnVars.fontFigure,
    fontSize: "10.5px",
    fontWeight: 700,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
  },
  word: {
    fill: "currentColor",
    fontFamily: awbrnVars.fontUi,
    fontSize: "11px",
    fontWeight: 700,
    letterSpacing: "0.12em",
    textAnchor: "middle",
  },
  figure: {
    fill: "currentColor",
    fontFamily: awbrnVars.fontUi,
    fontSize: "30px",
    fontWeight: 850,
    fontStretch: "62%",
    textAnchor: "middle",
  },
  figureAlone: {
    fontSize: "36px",
  },
});

import * as stylex from "@stylexjs/stylex";
import { useId } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/** A day as a postmark: two rings, ring text, a word and a figure. */
export function Postmark({
  figure,
  isNew = false,
  label,
  ringText,
  size,
  tone = "paper",
  word,
  xstyle,
}: {
  figure: number | string;
  /** Lands with a stamp, as when End turn stamps a new day. */
  isNew?: boolean;
  label: string;
  /** Repeated twice around the ring; keep it short and uppercase (`NOV 12 · 2026`). */
  ringText?: string;
  size?: number;
  tone?: "paper" | "ink";
  word?: string;
  xstyle?: stylex.StyleXStyles;
}) {
  const ringId = `postmark-ring-${useId().replace(/:/g, "")}`;
  const px = size ?? 64;

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
      {ringText ? (
        <text {...stylex.props(styles.ringText)}>
          <textPath href={`#${ringId}`} startOffset="0">
            {`${ringText} · ${ringText} · `}
          </textPath>
        </text>
      ) : null}
      {word ? (
        <text x="50" y="46" {...stylex.props(styles.word)}>
          {word}
        </text>
      ) : null}
      <text
        x="50"
        y={!word ? "61" : "72"}
        {...stylex.props(styles.figure, !word && styles.figureAlone)}
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

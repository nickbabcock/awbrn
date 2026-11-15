import * as stylex from "@stylexjs/stylex";
import { useHydrated } from "@tanstack/react-router";
import { useId } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

const formats = (timeZone?: string) => ({
  month: new Intl.DateTimeFormat("en-US", { month: "short", timeZone }),
  long: new Intl.DateTimeFormat("en-US", { dateStyle: "long", timeZone }),
});
const utcFormats = formats("UTC");
const localFormats = formats();

/**
 * The parts of a date a postmark carries: `12`, `NOV`, `2026` and `November
 * 12, 2026`, on the viewer's own calendar. The server knows no time zone, so
 * its page and the hydration that must match it stamp the UTC day first.
 */
export function usePostmarkDate(at: number | string) {
  const isLocal = useHydrated();
  const date = new Date(at);
  const format = isLocal ? localFormats : utcFormats;
  return {
    day: isLocal ? date.getDate() : date.getUTCDate(),
    month: format.month.format(date).toUpperCase(),
    year: isLocal ? date.getFullYear() : date.getUTCFullYear(),
    long: format.long.format(date),
  };
}

/**
 * A day as a postmark: two rings, ring text, a word and a figure. `arcText`
 * sets one word over the top arc in place of the ring text (`JOINED`), over a
 * month and a year.
 */
export function Postmark({
  arcText,
  figure,
  isNew = false,
  label,
  ringText,
  size,
  tone = "paper",
  word,
  xstyle,
}: {
  arcText?: string;
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
        <path
          d={
            arcText
              ? "M19,50 A31,31 0 0,1 81,50"
              : "M50,50 m-37,0 a37,37 0 1,1 74,0 a37,37 0 1,1 -74,0"
          }
          id={ringId}
        />
      </defs>
      <circle cx="50" cy="50" r="47" strokeWidth="2.6" {...stylex.props(styles.ring)} />
      <circle cx="50" cy="50" r="28.5" strokeWidth="1.6" {...stylex.props(styles.ring)} />
      {arcText ? (
        <text {...stylex.props(styles.ringText, styles.arcText)}>
          <textPath href={`#${ringId}`} startOffset="50%">
            {arcText}
          </textPath>
        </text>
      ) : ringText ? (
        <text {...stylex.props(styles.ringText)}>
          <textPath href={`#${ringId}`} startOffset="0">
            {`${ringText} · ${ringText} · `}
          </textPath>
        </text>
      ) : null}
      {word ? (
        <text x="50" y={arcText ? "43" : "46"} {...stylex.props(styles.word)}>
          {word}
        </text>
      ) : null}
      <text
        x="50"
        y={arcText ? "66" : !word ? "61" : "72"}
        {...stylex.props(
          styles.figure,
          !word && styles.figureAlone,
          Boolean(arcText) && styles.figureYear,
        )}
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
  // A year is four figures, where a day is two.
  figureYear: {
    fontSize: "22px",
    letterSpacing: "0.01em",
  },
  // One word over the top arc, at its narrowest so it fits the arc.
  arcText: {
    fontSize: "17px",
    fontStretch: "75%",
    letterSpacing: "0.04em",
    textAnchor: "middle",
  },
});

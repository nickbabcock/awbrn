import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/**
 * A figure: every counted or running number in AWBRN (funds, clocks, unit
 * counts, `1 / 2` seats, `302 / 581` replay position, `−1,000` shortfall,
 * terms such as `Live 5m +2m`). Martian Mono, tabular, at 87.5% width.
 *
 * Sizes: `sm` 12.5px (rows, tags, tile cells), `md` 15px, `lg` 22px (the
 * figure-lg: a clock in a strip, a rating). `tone` colors it for state; a
 * figure never names its unit when a sprite beside it already does.
 * Map names are never mono.
 *
 *   <Figure>1,000</Figure>
 *   <Figure size="lg" tone="warn">3m 42s</Figure>
 *
 * Inside your own StyleX, `figureStyles.figure` gives the face without the
 * element.
 */
export function Figure({
  as: Element = "span",
  children,
  size = "sm",
  tone = "inherit",
  xstyle,
}: {
  as?: "span" | "b" | "data" | "time";
  children: ReactNode;
  size?: "sm" | "md" | "lg";
  tone?: FigureTone;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <Element {...stylex.props(figureStyles.figure, sizes[size], tones[tone], xstyle)}>
      {children}
    </Element>
  );
}

export type FigureTone =
  | "inherit"
  | "primary"
  | "secondary"
  | "ok"
  | "warn"
  | "danger"
  | "on-ink"
  | "on-ink-strong"
  | "on-ink-secondary"
  | "warn-on-ink"
  | "gold";

export const figureStyles = stylex.create({
  figure: {
    fontFamily: awbrnVars.fontFigure,
    fontStretch: "87.5%",
    fontVariantNumeric: "tabular-nums",
    letterSpacing: 0,
    lineHeight: 1.1,
    whiteSpace: "nowrap",
  },
});

const sizes = stylex.create({
  sm: { fontSize: awbrnVars.text12_5, fontWeight: 500 },
  md: { fontSize: awbrnVars.text15, fontWeight: 600 },
  lg: { fontSize: "22px", fontWeight: 600, letterSpacing: "-0.02em", lineHeight: 1 },
});

const tones = stylex.create({
  inherit: {},
  primary: { color: colorVars["--color-text-primary"] },
  secondary: { color: colorVars["--color-text-secondary"] },
  ok: { color: awbrnVars.ok },
  warn: { color: awbrnVars.warn },
  danger: { color: awbrnVars.danger },
  "on-ink": { color: awbrnVars.textOnInk },
  "on-ink-strong": { color: awbrnVars.textOnInkStrong },
  "on-ink-secondary": { color: awbrnVars.textOnInkSecondary },
  "warn-on-ink": { color: awbrnVars.warnFill },
  gold: { color: awbrnVars.gold },
});

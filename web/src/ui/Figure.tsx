import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/** A counted or running number. `figureStyles.figure` gives the face without the element. */
export function Figure({
  children,
  size = "sm",
  tone = "inherit",
  xstyle,
}: {
  children: ReactNode;
  size?: "sm" | "md" | "lg";
  tone?: FigureTone;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <span {...stylex.props(figureStyles.figure, sizes[size], tones[tone], xstyle)}>{children}</span>
  );
}

export type FigureTone =
  | "inherit"
  | "primary"
  | "secondary"
  | "ok"
  | "warn"
  | "danger"
  | "on-ink-strong"
  | "on-ink-secondary"
  | "warn-on-ink";

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
  "on-ink-strong": { color: awbrnVars.textOnInkStrong },
  "on-ink-secondary": { color: awbrnVars.textOnInkSecondary },
  "warn-on-ink": { color: awbrnVars.warnFill },
});

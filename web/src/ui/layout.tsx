import * as stylex from "@stylexjs/stylex";
import type { ElementType, ReactNode } from "react";
import { tokens } from "./theme.stylex";
import { sx, type XStyle } from "./stylex";

export type Space = "xs" | "sm" | "md" | "lg" | "xl";
export type Width = "content" | "wide" | "full";

const riseIn = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(18px)" },
  to: { opacity: 1, transform: "translateY(0)" },
});

const gapMap: Record<Space, XStyle> = stylex.create({
  xs: { gap: tokens.space2 },
  sm: { gap: tokens.space3 },
  md: { gap: tokens.space4 },
  lg: { gap: tokens.space6 },
  xl: { gap: tokens.space8 },
});

const widthMap: Record<Width, XStyle> = stylex.create({
  content: {
    width: "100%",
    maxWidth: tokens.maxWidth,
    marginInline: "auto",
  },
  wide: {
    width: "100%",
    maxWidth: tokens.maxWidthWide,
    marginInline: "auto",
  },
  full: {
    width: "100%",
  },
});

const styles = stylex.create({
  page: {
    minHeight: `calc(100vh - ${tokens.navHeight})`,
    paddingBlock: "clamp(24px, 5vw, 40px)",
    paddingInline: "clamp(16px, 4vw, 40px)",
  },
  section: {
    width: "100%",
    animationDuration: "320ms",
    animationFillMode: "both",
    animationName: riseIn,
    "@media (prefers-reduced-motion: reduce)": {
      animationName: "none",
      animationDuration: "0ms",
    },
  },
  stack: {
    display: "flex",
    flexDirection: "column",
  },
  inline: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
  },
});

export function Page({
  children,
  width = "content",
  xstyle,
}: {
  children: ReactNode;
  width?: Width;
  xstyle?: XStyle;
}) {
  return <main {...sx(styles.page, widthMap[width], xstyle)}>{children}</main>;
}

export function Section({ children, xstyle }: { children: ReactNode; xstyle?: XStyle }) {
  return <section {...sx(styles.section, xstyle)}>{children}</section>;
}

export function Stack({
  as,
  children,
  gap = "md",
  xstyle,
}: {
  as?: ElementType;
  children: ReactNode;
  gap?: Space;
  xstyle?: XStyle;
}) {
  const Component = as ?? "div";
  return <Component {...sx(styles.stack, gapMap[gap], xstyle)}>{children}</Component>;
}

export function Inline({
  as,
  children,
  gap = "md",
  xstyle,
}: {
  as?: ElementType;
  children: ReactNode;
  gap?: Space;
  xstyle?: XStyle;
}) {
  const Component = as ?? "div";
  return <Component {...sx(styles.inline, gapMap[gap], xstyle)}>{children}</Component>;
}

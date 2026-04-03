import { Link } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { Fragment } from "react";
import type { ComponentPropsWithoutRef, ElementType, ReactNode } from "react";
import { tokens } from "./theme.stylex";
import { sx, sxClassName, type XStyle } from "./stylex";

export type TextTone =
  | "strong"
  | "default"
  | "muted"
  | "danger"
  | "success"
  | "inverse"
  | "inverseMuted";

const headingStyles = stylex.create({
  display: {
    margin: 0,
    fontFamily: tokens.fontPixel,
    fontSize: tokens.displayMd,
    lineHeight: tokens.leadingPixel,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  lg: {
    margin: 0,
    fontFamily: tokens.fontPixel,
    fontSize: tokens.displaySm,
    lineHeight: tokens.leadingPixel,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  md: {
    margin: 0,
    fontFamily: tokens.fontPixel,
    fontSize: tokens.textXs,
    lineHeight: tokens.leadingPixel,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
});

const kickerStyles = stylex.create({
  base: {
    margin: 0,
    color: tokens.inkSoft,
    fontFamily: tokens.fontPixel,
    fontSize: 9,
    letterSpacing: tokens.trackingWide,
    lineHeight: 1.8,
    textTransform: "uppercase",
  },
});

const textStyles = stylex.create({
  base: {
    margin: 0,
    fontFamily: tokens.fontBody,
    color: tokens.ink,
    lineHeight: tokens.leadingBody,
  },
  strong: { color: tokens.inkStrong },
  inverse: { color: tokens.onDarkStrong },
  inverseMuted: { color: tokens.onDarkMuted },
  muted: { color: tokens.inkMuted },
  danger: { color: tokens.danger },
  success: { color: tokens.success },
  sm: { fontSize: tokens.textSm },
  md: { fontSize: tokens.textBase },
  lg: { fontSize: tokens.textLg },
});

const codeStyles = stylex.create({
  base: {
    fontFamily: tokens.fontPixel,
    fontSize: 10,
    color: tokens.inkStrong,
    backgroundColor: tokens.panelBg,
    borderRadius: tokens.radius1,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    paddingInline: 6,
    paddingBlock: 2,
  },
});

const wordmarkStyles = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    gap: 2,
    fontFamily: tokens.fontPixel,
    fontSize: "clamp(20px, 3vw, 44px)",
    lineHeight: 1,
    letterSpacing: tokens.trackingPixel,
    textDecoration: "none",
    textTransform: "uppercase",
  },
  nav: {
    fontSize: 16,
  },
  shadow: {
    textShadow: "2px 2px 0 #000, 3px 3px 0 rgba(0,0,0,0.3)",
  },
  os: { color: "#ff4f4e" },
  bm: { color: "#466efe" },
  ge: { color: "#3dc22d" },
  yc: { color: "#9f8f00" },
  bh: { color: "#74598a" },
});

const headingSizeMap: Record<"display" | "lg" | "md", XStyle> = {
  display: headingStyles.display,
  lg: headingStyles.lg,
  md: headingStyles.md,
};

const textToneMap: Record<TextTone, XStyle> = {
  strong: textStyles.strong,
  default: null,
  muted: textStyles.muted,
  danger: textStyles.danger,
  success: textStyles.success,
  inverse: textStyles.inverse,
  inverseMuted: textStyles.inverseMuted,
};

const textSizeMap: Record<"sm" | "md" | "lg", XStyle> = {
  sm: textStyles.sm,
  md: textStyles.md,
  lg: textStyles.lg,
};

const wordmarkSizeMap: Record<"display" | "nav", XStyle> = {
  display: null,
  nav: wordmarkStyles.nav,
};

type TextProps<E extends ElementType> = {
  as?: E;
  children: ReactNode;
  tone?: TextTone;
  size?: "sm" | "md" | "lg";
  xstyle?: XStyle;
} & Omit<ComponentPropsWithoutRef<E>, "as" | "children" | "color" | "size">;

export function Heading({
  as,
  children,
  size = "lg",
  xstyle,
}: {
  as?: ElementType;
  children: ReactNode;
  size?: "display" | "lg" | "md";
  xstyle?: XStyle;
}) {
  const Component = as ?? (size === "display" ? "h1" : "h2");
  return <Component {...sx(headingSizeMap[size], xstyle)}>{children}</Component>;
}

export function Kicker({ children, xstyle }: { children: ReactNode; xstyle?: XStyle }) {
  return <p {...sx(kickerStyles.base, xstyle)}>{children}</p>;
}

export function Text<E extends ElementType = "p">({
  as,
  children,
  tone = "default",
  size = "md",
  xstyle,
  ...props
}: TextProps<E>) {
  const Component = (as ?? "p") as ElementType;
  return (
    <Component {...props} {...sx(textStyles.base, textToneMap[tone], textSizeMap[size], xstyle)}>
      {children}
    </Component>
  );
}

export function CodeText({ children, xstyle }: { children: ReactNode; xstyle?: XStyle }) {
  return <code {...sx(codeStyles.base, xstyle)}>{children}</code>;
}

export function Wordmark({
  href,
  size = "display",
  shadow = false,
}: {
  href?: string;
  size?: "display" | "nav";
  shadow?: boolean;
}) {
  const content = (
    <Fragment>
      <span {...sx(wordmarkStyles.os)}>A</span>
      <span {...sx(wordmarkStyles.bm)}>W</span>
      <span {...sx(wordmarkStyles.ge)}>B</span>
      <span {...sx(wordmarkStyles.yc)}>R</span>
      <span {...sx(wordmarkStyles.bh)}>N</span>
    </Fragment>
  );

  if (href) {
    return (
      <Link
        aria-label="AWBRN home"
        className={sxClassName(
          wordmarkStyles.base,
          wordmarkSizeMap[size],
          shadow && wordmarkStyles.shadow,
        )}
        to={href}
      >
        {content}
      </Link>
    );
  }

  return (
    <span
      aria-label="AWBRN"
      {...sx(wordmarkStyles.base, wordmarkSizeMap[size], shadow && wordmarkStyles.shadow)}
    >
      {content}
    </span>
  );
}

import * as stylex from "@stylexjs/stylex";
import type { ElementType, ReactNode } from "react";
import { tokens } from "./theme.stylex";
import { sx, type XStyle } from "./stylex";
import type { Tone } from "./shared-types";
import { Heading, Kicker, Text } from "./typography";
import { Inline, Stack } from "./layout";

const noticeToneMap = stylex.create({
  neutral: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    color: tokens.inkStrong,
  },
  brand: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: tokens.brandSoft,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    color: tokens.brandHover,
  },
  success: {
    borderColor: tokens.success,
    backgroundColor: "rgba(26, 158, 63, 0.18)",
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    color: tokens.successHover,
  },
  danger: {
    borderColor: tokens.hazardInk,
    backgroundColor: tokens.hazardYellow,
    backgroundImage: tokens.hazardStripe,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    color: tokens.hazardInk,
  },
});

const frameStyles = stylex.create({
  base: {
    borderWidth: 3,
    borderStyle: "solid",
    borderRadius: tokens.radius3,
    overflow: "hidden",
    position: "relative",
  },
  panel: {
    backgroundColor: tokens.panelBg,
    borderColor: tokens.strokeHeavy,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardLg}`,
    color: tokens.inkStrong,
  },
  inset: {
    backgroundColor: tokens.panelInset,
    borderColor: tokens.strokeBase,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    color: tokens.inkStrong,
  },
  chrome: {
    backgroundColor: tokens.chromeBgElevated,
    borderColor: tokens.chromeBorder,
    boxShadow: `${tokens.highlightInsetChrome}, ${tokens.shadowHardLg}`,
    color: tokens.onDarkStrong,
  },
  padSm: { padding: tokens.space3 },
  padMd: { padding: tokens.space4 },
  padLg: { padding: tokens.space6 },
});

const insetStyles = stylex.create({
  base: {
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    padding: tokens.space4,
  },
});

const ruleStyles = stylex.create({
  base: {
    inlineSize: "100%",
    blockSize: 2,
    backgroundColor: tokens.strokeBase,
  },
});

const noticeStyles = stylex.create({
  base: {
    borderWidth: 3,
    borderStyle: "solid",
    borderRadius: tokens.radius2,
    paddingInline: tokens.space4,
    paddingBlock: tokens.space3,
    backgroundColor: tokens.panelRaised,
  },
});

const emptyStateStyles = stylex.create({
  base: {
    minHeight: 220,
    display: "flex",
    flexDirection: "column",
    justifyContent: "center",
  },
});

const surfaceMap: Record<"panel" | "inset" | "chrome", XStyle> = {
  panel: frameStyles.panel,
  inset: frameStyles.inset,
  chrome: frameStyles.chrome,
};

const paddingMap: Record<"sm" | "md" | "lg", XStyle> = {
  sm: frameStyles.padSm,
  md: frameStyles.padMd,
  lg: frameStyles.padLg,
};

export function Frame({
  as,
  children,
  surface = "panel",
  padding = "md",
  xstyle,
}: {
  as?: ElementType;
  children: ReactNode;
  surface?: "panel" | "inset" | "chrome";
  padding?: "sm" | "md" | "lg" | "none";
  xstyle?: XStyle;
}) {
  const Component = as ?? "div";
  const padStyle = padding === "none" ? null : paddingMap[padding];

  return (
    <Component {...sx(frameStyles.base, surfaceMap[surface], padStyle, xstyle)}>
      {children}
    </Component>
  );
}

export function Inset({ children, xstyle }: { children: ReactNode; xstyle?: XStyle }) {
  return <div {...sx(insetStyles.base, xstyle)}>{children}</div>;
}

export function Rule({ xstyle }: { xstyle?: XStyle }) {
  return <div aria-hidden="true" {...sx(ruleStyles.base, xstyle)} />;
}

export function Notice({
  children,
  tone = "neutral",
  xstyle,
}: {
  children: ReactNode;
  tone?: Tone;
  xstyle?: XStyle;
}) {
  return <div {...sx(noticeStyles.base, noticeToneMap[tone], xstyle)}>{children}</div>;
}

export function EmptyState({
  kicker,
  title,
  description,
  actions,
  xstyle,
}: {
  kicker?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  xstyle?: XStyle;
}) {
  return (
    <Stack gap="md" xstyle={[emptyStateStyles.base, xstyle]}>
      {kicker ? <Kicker>{kicker}</Kicker> : null}
      <Heading size="lg">{title}</Heading>
      {description ? <Text size="lg">{description}</Text> : null}
      {actions ? <Inline gap="sm">{actions}</Inline> : null}
    </Stack>
  );
}

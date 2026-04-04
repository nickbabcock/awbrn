import { Button as BaseButton } from "@base-ui/react/button";
import { createLink } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { forwardRef } from "react";
import type { ComponentPropsWithoutRef, ReactNode } from "react";
import { tokens } from "./theme.stylex";
import { sx, sxClassName, type XStyle } from "./stylex";
import type { Tone } from "./shared-types";

export type ButtonVariant = "solid" | "outline" | "ghost";
export type ButtonSize = "sm" | "md" | "lg";

const badgeStyles = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    minHeight: 28,
    paddingInline: tokens.space3,
    borderRadius: tokens.radiusRound,
    borderWidth: 2,
    borderStyle: "solid",
    fontFamily: tokens.fontPixel,
    fontSize: 9,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  },
  neutral: {
    backgroundColor: tokens.panelRaised,
    borderColor: tokens.strokeHeavy,
    color: tokens.inkStrong,
  },
  brand: {
    backgroundColor: tokens.brandSoft,
    borderColor: tokens.strokeHeavy,
    color: tokens.brandHover,
  },
  success: {
    backgroundColor: "rgba(26, 158, 63, 0.22)",
    borderColor: tokens.success,
    color: tokens.successHover,
  },
  danger: {
    backgroundColor: "rgba(182, 74, 57, 0.18)",
    borderColor: tokens.strokeHeavy,
    color: tokens.dangerHover,
  },
});

const buttonStyles = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: tokens.space2,
    minHeight: 40,
    paddingInline: tokens.space4,
    borderWidth: 3,
    borderStyle: "solid",
    borderRadius: tokens.radius2,
    fontFamily: tokens.fontPixel,
    letterSpacing: "0.06em",
    textDecoration: "none",
    textTransform: "uppercase",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, background-color, border-color, color, opacity",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
      ":disabled": "translateY(0)",
    },
    boxShadow: {
      default: `${tokens.highlightInset}, ${tokens.shadowHardMd}`,
      ":hover": `${tokens.highlightInset}, ${tokens.shadowHardLg}`,
      ":active": tokens.highlightInset,
      ":disabled": `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    },
    opacity: {
      default: 1,
      ":disabled": 0.55,
    },
    cursor: {
      default: "pointer",
      ":disabled": "not-allowed",
    },
  },
  solidNeutral: {
    backgroundColor: tokens.panelRaised,
    borderColor: tokens.strokeHeavy,
    color: tokens.inkStrong,
  },
  solidBrand: {
    backgroundColor: tokens.brand,
    borderColor: tokens.strokeHeavy,
    color: tokens.onDarkStrong,
  },
  solidSuccess: {
    backgroundColor: tokens.success,
    borderColor: tokens.strokeHeavy,
    color: tokens.onDarkStrong,
  },
  solidDanger: {
    backgroundColor: tokens.hazardYellow,
    backgroundImage: tokens.hazardStripe,
    borderColor: tokens.hazardInk,
    color: tokens.hazardInk,
  },
  outline: {
    backgroundColor: tokens.panelRaised,
  },
  outlineNeutral: {
    borderColor: tokens.strokeHeavy,
    color: tokens.inkStrong,
  },
  outlineBrand: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: tokens.brandSoft,
    color: tokens.brandHover,
  },
  outlineSuccess: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: "rgba(47, 142, 69, 0.18)",
    color: tokens.successHover,
  },
  outlineDanger: {
    borderColor: tokens.hazardInk,
    backgroundColor: tokens.hazardYellow,
    color: tokens.hazardInk,
  },
  ghostNeutral: {
    borderColor: tokens.strokeHeavy,
    color: tokens.inkStrong,
    backgroundColor: "rgba(255, 255, 255, 0.35)",
  },
  ghostBrand: {
    borderColor: tokens.strokeHeavy,
    color: tokens.brand,
    backgroundColor: "rgba(255, 208, 157, 0.4)",
  },
  ghostSuccess: {
    borderColor: tokens.strokeHeavy,
    color: tokens.success,
    backgroundColor: "rgba(47, 142, 69, 0.16)",
  },
  ghostDanger: {
    borderColor: tokens.hazardInk,
    color: tokens.hazardInk,
    backgroundColor: tokens.hazardYellow,
  },
  sm: {
    minHeight: 34,
    fontSize: 9,
    paddingInline: tokens.space3,
  },
  md: {
    minHeight: 42,
    fontSize: tokens.textXs,
    paddingInline: tokens.space4,
  },
  lg: {
    minHeight: 48,
    fontSize: 11,
    paddingInline: tokens.space5,
  },
  full: {
    width: "100%",
  },
  link: {
    textDecoration: "none",
  },
});

const badgeToneMap: Record<Tone, XStyle> = {
  neutral: badgeStyles.neutral,
  brand: badgeStyles.brand,
  success: badgeStyles.success,
  danger: badgeStyles.danger,
};

const variantBaseMap: Record<ButtonVariant, XStyle> = {
  solid: null,
  outline: buttonStyles.outline,
  ghost: null,
};

const toneMap: Record<ButtonVariant, Record<Tone, XStyle>> = {
  solid: {
    neutral: buttonStyles.solidNeutral,
    brand: buttonStyles.solidBrand,
    success: buttonStyles.solidSuccess,
    danger: buttonStyles.solidDanger,
  },
  outline: {
    neutral: buttonStyles.outlineNeutral,
    brand: buttonStyles.outlineBrand,
    success: buttonStyles.outlineSuccess,
    danger: buttonStyles.outlineDanger,
  },
  ghost: {
    neutral: buttonStyles.ghostNeutral,
    brand: buttonStyles.ghostBrand,
    success: buttonStyles.ghostSuccess,
    danger: buttonStyles.ghostDanger,
  },
};

const sizeMap: Record<ButtonSize, XStyle> = {
  sm: buttonStyles.sm,
  md: buttonStyles.md,
  lg: buttonStyles.lg,
};

export function Badge({
  children,
  tone = "neutral",
  xstyle,
}: {
  children: ReactNode;
  tone?: Tone;
  xstyle?: XStyle;
}) {
  return <span {...sx(badgeStyles.base, badgeToneMap[tone], xstyle)}>{children}</span>;
}

export function Button({
  children,
  variant = "solid",
  tone = "neutral",
  size = "md",
  loading = false,
  fullWidth = false,
  xstyle,
  disabled,
  ...props
}: Omit<ComponentPropsWithoutRef<typeof BaseButton>, "className"> & {
  children: ReactNode;
  variant?: ButtonVariant;
  tone?: Tone;
  size?: ButtonSize;
  loading?: boolean;
  fullWidth?: boolean;
  xstyle?: XStyle;
}) {
  return (
    <BaseButton
      {...props}
      disabled={disabled || loading}
      {...sx(
        buttonStyles.base,
        variantBaseMap[variant],
        sizeMap[size],
        toneMap[variant][tone],
        fullWidth && buttonStyles.full,
        xstyle,
      )}
    >
      {children}
    </BaseButton>
  );
}

const ButtonLinkAnchor = forwardRef<
  HTMLAnchorElement,
  Omit<ComponentPropsWithoutRef<"a">, "className" | "style"> & {
    variant?: ButtonVariant;
    tone?: Tone;
    size?: ButtonSize;
    xstyle?: XStyle;
  }
>(({ children, variant = "solid", tone = "neutral", size = "md", xstyle, ...props }, ref) => (
  <a
    ref={ref}
    {...props}
    className={sxClassName(
      buttonStyles.base,
      buttonStyles.link,
      variantBaseMap[variant],
      sizeMap[size],
      toneMap[variant][tone],
      xstyle,
    )}
  >
    {children}
  </a>
));

export const ButtonLink = createLink(ButtonLinkAnchor);

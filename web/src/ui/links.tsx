import { createLink } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { forwardRef } from "react";
import type { ComponentPropsWithoutRef } from "react";
import { tokens } from "./theme.stylex";
import { sxClassName } from "./stylex";

const styles = stylex.create({
  link: {
    color: {
      default: tokens.brand,
      ":hover": tokens.brandHover,
    },
    textDecoration: "none",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "color",
  },
  recovery: {
    color: tokens.brand,
    fontFamily: tokens.fontPixel,
    fontSize: 10,
    letterSpacing: "0.06em",
    textDecoration: "none",
    textTransform: "uppercase",
  },
});

export const recoveryLinkStyle = styles.recovery;

const AppLinkAnchor = forwardRef<
  HTMLAnchorElement,
  Omit<ComponentPropsWithoutRef<"a">, "className" | "style">
>((props, ref) => <a ref={ref} {...props} className={sxClassName(styles.link)} />);

export const AppLink = createLink(AppLinkAnchor);

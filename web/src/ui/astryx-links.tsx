import * as stylex from "@stylexjs/stylex";
import { durationVars } from "@astryxdesign/core/theme/tokens.stylex";
import { Link as AstryxLink, type LinkProps as AstryxLinkProps } from "@astryxdesign/core/Link";
import { ListItem, type ListItemProps } from "@astryxdesign/core/List";
import {
  TopNavHeading,
  TopNavItem,
  type TopNavHeadingProps,
  type TopNavItemProps,
} from "@astryxdesign/core/TopNav";
import { createLink } from "@tanstack/react-router";
import { forwardRef, type Ref } from "react";
import { Button, type ButtonProps } from "#/ui/Button.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

type AstryxButtonLinkHostProps = Omit<ButtonProps, "as" | "href" | "ref"> & {
  href?: string;
};

const AstryxButtonLinkHost = forwardRef<HTMLAnchorElement, AstryxButtonLinkHostProps>(
  function AstryxButtonLinkHost(props, ref) {
    return <Button {...props} as="a" ref={ref as Ref<HTMLButtonElement>} />;
  },
);

/**
 * Astryx Button styling with TanStack Router's type-safe navigation API.
 * Use this for internal routes; use Button with href for external URLs.
 */
export const RouterButton = createLink(AstryxButtonLinkHost);

type AstryxTextLinkHostProps = Omit<AstryxLinkProps, "as" | "href" | "ref"> & {
  href?: string;
};

const AstryxTextLinkHost = forwardRef<HTMLAnchorElement, AstryxTextLinkHostProps>(
  function AstryxTextLinkHost(props, ref) {
    return <AstryxLink {...props} as="a" ref={ref} />;
  },
);

export const RouterTextLink = createLink(AstryxTextLinkHost);

type AstryxListItemLinkHostProps = Omit<ListItemProps, "href" | "ref"> & {
  disabled?: boolean;
  href?: string;
};

const AstryxListItemLinkHost = forwardRef<HTMLLIElement, AstryxListItemLinkHostProps>(
  function AstryxListItemLinkHost({ disabled: _disabled, ...props }, ref) {
    return <ListItem {...props} ref={ref} />;
  },
);

export const RouterListItem = createLink(AstryxListItemLinkHost);

type AstryxTopNavItemLinkHostProps = Omit<TopNavItemProps, "as" | "href" | "ref"> & {
  href?: string;
};

const AstryxTopNavItemLinkHost = forwardRef<HTMLAnchorElement, AstryxTopNavItemLinkHostProps>(
  function AstryxTopNavItemLinkHost({ xstyle, ...props }, ref) {
    return (
      <TopNavItem
        {...props}
        as="a"
        ref={ref}
        xstyle={[styles.topNavItem, styles.reducedMotion, xstyle]}
      />
    );
  },
);

export const RouterTopNavItem = createLink(AstryxTopNavItemLinkHost);

type AstryxTopNavHeadingLinkHostProps = Omit<
  TopNavHeadingProps,
  "as" | "headingHref" | "href" | "ref"
> & {
  href?: string;
};

const AstryxTopNavHeadingLinkHost = forwardRef<HTMLAnchorElement, AstryxTopNavHeadingLinkHostProps>(
  function AstryxTopNavHeadingLinkHost({ href, ...props }, ref) {
    return <TopNavHeading {...props} as="a" headingHref={href} ref={ref} />;
  },
);

export const RouterTopNavHeading = createLink(AstryxTopNavHeadingLinkHost);

const styles = stylex.create({
  topNavItem: {
    boxShadow: {
      default: null,
      ":active": "none",
    },
    transform: {
      default: null,
      ":active": `translate(${awbrnVars.offsetControlPressed}, ${awbrnVars.offsetControlPressed})`,
    },
    transitionDuration: {
      default: null,
      ":active": durationVars["--duration-fast-min"],
    },
  },
  reducedMotion: {
    transform: {
      default: null,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
});

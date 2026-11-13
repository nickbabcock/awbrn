import { Link as AstryxLink, type LinkProps as AstryxLinkProps } from "@astryxdesign/core/Link";
import { createLink } from "@tanstack/react-router";
import { forwardRef, type Ref } from "react";
import { Button, type ButtonProps } from "#/ui/Button.tsx";

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

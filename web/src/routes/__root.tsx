/// <reference types="vite/client" />
import type { QueryClient } from "@tanstack/react-query";
import {
  Link,
  HeadContent,
  Outlet,
  Scripts,
  createRootRouteWithContext,
} from "@tanstack/react-router";
import { LinkProvider } from "@astryxdesign/core/Link";
import { Theme } from "@astryxdesign/core/theme";
import { forwardRef, type ComponentPropsWithoutRef, type ReactNode } from "react";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { DefaultCatchBoundary } from "#/components/DefaultCatchBoundary.tsx";
import { NotFound } from "#/components/NotFound.tsx";
import { GameRuntimeProvider } from "#/engine/runtime_context.tsx";
import { Layout } from "#/layouts/Layout.tsx";
import { DevStyleXInject } from "#/styles/DevStyleXInject.tsx";
import resetCss from "#/styles/reset.css?url";
import { awbrnTheme } from "#/themes/awbrn.js";

export const Route = createRootRouteWithContext<{
  queryClient: QueryClient;
}>()({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "AWBRN" },
    ],
    links: [{ rel: "stylesheet", href: resetCss }],
  }),
  loader: ({ context }) => context.queryClient.ensureQueryData(sessionQueryOptions()),
  errorComponent: DefaultCatchBoundary,
  notFoundComponent: () => <NotFound />,
  component: RootComponent,
  shellComponent: RootDocument,
});

function RootComponent() {
  return (
    <Theme mode="light" theme={awbrnTheme}>
      <LinkProvider component={RouterLink}>
        <GameRuntimeProvider>
          <Layout>
            <Outlet />
          </Layout>
        </GameRuntimeProvider>
      </LinkProvider>
    </Theme>
  );
}

type RouterLinkProps = Omit<ComponentPropsWithoutRef<"a">, "href"> & {
  href?: string;
};

const RouterLink = forwardRef<HTMLAnchorElement, RouterLinkProps>(function RouterLink(
  { href = "/", ...props },
  ref,
) {
  return <Link {...props} ref={ref} to={href} />;
});

function RootDocument({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <head>
        <HeadContent />
        {import.meta.env.DEV ? <DevStyleXInject /> : null}
      </head>
      <body>
        <main id="app-root">{children}</main>
        <Scripts />
      </body>
    </html>
  );
}

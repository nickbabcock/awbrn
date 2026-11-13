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
import { matchesAwaitingQueryOptions } from "#/matches/matches.queries.ts";
import { DevStyleXInject } from "#/styles/DevStyleXInject.tsx";
import resetCss from "#/styles/reset.css?url";
import { awbrnTheme } from "#/themes/awbrn.js";

export const Route = createRootRouteWithContext<{
  queryClient: QueryClient;
}>()({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      // viewport-fit=cover lets the bars run under the notch and the home
      // indicator; the shell pads them with the safe-area insets.
      { name: "viewport", content: "width=device-width, initial-scale=1, viewport-fit=cover" },
      // The browser's own chrome wears the top bar's ink.
      { name: "theme-color", content: "#101b3a" },
      { name: "color-scheme", content: "light" },
      { title: "AWBRN" },
    ],
    links: [
      { rel: "stylesheet", href: resetCss },
      // The brand mark, until the shell draws the waiting count on it.
      { rel: "icon", href: "/favicon.svg", type: "image/svg+xml" },
    ],
  }),
  loader: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    // The My games count paints with the first frame, on every page and width,
    // rather than popping in once the client asks for it. A count already in
    // the cache stands; the shell's own query keeps it current.
    if (session) {
      await context.queryClient.prefetchQuery({
        ...matchesAwaitingQueryOptions(false),
        staleTime: Number.POSITIVE_INFINITY,
      });
    }
    return session;
  },
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
        {/* AppShell renders the page's one <main>; this is only the mount, an unstyled
            block outside the component tree, so no layout component fits. */}
        <div id="app-root">{children}</div>
        <Scripts />
      </body>
    </html>
  );
}

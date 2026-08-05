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
import ImpeccableLiveRoot from "../impeccable/ImpeccableLiveRoot";

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
        {/* The direction contract this design is built against. React cannot emit a
            comment node, so it rides in a hidden element that survives the build. */}
        <div
          hidden
          dangerouslySetInnerHTML={{
            __html: `<!--
awbrn:direction-contract
THESIS: A game client that looks like the game. Refuses the neutral SaaS dashboard
that every AWBW tool defaults to.
OWN-WORLD: Daylight only. Open-sky ground under a fixed tile grid; cream menu
panels outlined in one black, cast onto the terrain by a hard pixel shadow plus a
soft landing blur. Command orange for action, army hues for identity. Three voices:
Bungee is box art, Silkscreen is the HUD, Nunito is the briefing.
STORY: A player recognizes the game before reading a word, then finds the match or
replay they came for without the look getting in the way.
FIRST VIEWPORT: Cream nav bar on a black rule, wordmark left in Bungee, tabs as
Silkscreen HUD labels with the selected tab wearing the orange cursor. Page content
in outlined menu panels over sky.
FORM: Pinned by the user, not rolled: "the playful retro look of advance wars",
drawn from four surfaces of the source game at once.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish
review, the verdict, and DESIGN.md
-->`,
          }}
        />
        <main id="app-root">{children}</main>
        {/* impeccable-live-tanstack-start */}
        <ImpeccableLiveRoot />
        {/* impeccable-live-tanstack-end */}
        <Scripts />
      </body>
    </html>
  );
}

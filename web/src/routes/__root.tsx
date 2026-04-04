/// <reference types="vite/client" />
import { HeadContent, Outlet, Scripts, createRootRoute } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { getSessionFn } from "../auth/auth.functions";
import { DefaultCatchBoundary } from "../components/DefaultCatchBoundary";
import { NotFound } from "../components/NotFound";
import { Layout } from "../layouts/Layout";
import { DevStyleXInject } from "../styles/DevStyleXInject";
import resetCss from "../styles/reset.css?url";
import { appTheme, rootStyles } from "../ui/theme.stylex";

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "AWBRN" },
    ],
    links: [{ rel: "stylesheet", href: resetCss }],
  }),
  loader: () => getSessionFn(),
  errorComponent: DefaultCatchBoundary,
  notFoundComponent: () => <NotFound />,
  component: RootComponent,
  shellComponent: RootDocument,
});

function RootComponent() {
  return (
    <Layout>
      <Outlet />
    </Layout>
  );
}

function RootDocument({ children }: { children: ReactNode }) {
  return (
    <html lang="en" {...stylex.props(appTheme, rootStyles.html)}>
      <head>
        <HeadContent />
        {import.meta.env.DEV ? <DevStyleXInject /> : null}
      </head>
      <body {...stylex.props(rootStyles.body)}>
        <div id="app-root" {...stylex.props(rootStyles.appRoot)}>
          {children}
        </div>
        <Scripts />
      </body>
    </html>
  );
}

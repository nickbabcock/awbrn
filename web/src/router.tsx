import { createRouter } from "@tanstack/react-router";
import { DefaultCatchBoundary } from "#/components/DefaultCatchBoundary.tsx";
import { NotFound } from "#/components/NotFound.tsx";
import { routeTree } from "./routeTree.gen";

export function getRouter() {
  return createRouter({
    routeTree,
    defaultPreload: "intent",
    defaultErrorComponent: DefaultCatchBoundary,
    defaultNotFoundComponent: () => <NotFound />,
    scrollRestoration: true,
  });
}

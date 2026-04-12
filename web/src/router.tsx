import { QueryClient } from "@tanstack/react-query";
import { createRouter } from "@tanstack/react-router";
import { setupRouterSsrQueryIntegration } from "@tanstack/react-router-ssr-query";
import { DefaultCatchBoundary } from "#/components/DefaultCatchBoundary.tsx";
import { NotFound } from "#/components/NotFound.tsx";
import { routeTree } from "./routeTree.gen";

export function getRouter() {
  const queryClient: QueryClient = new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 60_000,
        refetchOnWindowFocus: false,
        refetchOnReconnect: (): boolean => !queryClient.isMutating(),
      },
    },
  });

  const router = createRouter({
    routeTree,
    defaultPreload: "intent",
    defaultErrorComponent: DefaultCatchBoundary,
    defaultNotFoundComponent: () => <NotFound />,
    scrollRestoration: true,
    context: { queryClient },
  });

  setupRouterSsrQueryIntegration({
    router,
    queryClient,
  });

  return router;
}

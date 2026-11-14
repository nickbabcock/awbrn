import { createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { AuthPage } from "#/auth/AuthPage.tsx";

type AuthSearch = {
  mode?: "register";
};

export const Route = createFileRoute("/auth")({
  validateSearch: (search: Record<string, unknown>): AuthSearch => ({
    mode: search.mode === "register" ? "register" : undefined,
  }),
  // The form belongs to the guest shell: a signed-in viewer goes home.
  beforeLoad: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    if (session) {
      throw redirect({ replace: true, to: "/" });
    }
  },
  component: AuthRoute,
});

function AuthRoute() {
  const search = Route.useSearch();
  return <AuthPage isRegister={search.mode === "register"} />;
}

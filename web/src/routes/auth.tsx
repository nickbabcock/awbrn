import { createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { AuthPage } from "#/auth/AuthPage.tsx";
import { signInReturn } from "#/auth/sign_in_redirect.ts";

type AuthSearch = {
  mode?: "register";
  /** Where to go once signed in. */
  redirect?: string;
};

export const Route = createFileRoute("/auth")({
  validateSearch: (search: Record<string, unknown>): AuthSearch => ({
    mode: search.mode === "register" ? "register" : undefined,
    redirect: signInReturn(search.redirect),
  }),
  // The form belongs to the guest shell: a signed-in viewer goes home.
  beforeLoad: async ({ context, search }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    if (session) {
      throw redirect({ href: search.redirect ?? "/", replace: true });
    }
  },
  component: AuthRoute,
});

function AuthRoute() {
  const search = Route.useSearch();
  return <AuthPage isRegister={search.mode === "register"} returnTo={search.redirect} />;
}

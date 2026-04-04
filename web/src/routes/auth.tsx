import { createFileRoute } from "@tanstack/react-router";
import { AuthPage } from "#/auth/AuthPage.tsx";

type AuthSearch = {
  mode?: "register";
};

export const Route = createFileRoute("/auth")({
  validateSearch: (search: Record<string, unknown>): AuthSearch => ({
    mode: search.mode === "register" ? "register" : undefined,
  }),
  component: AuthRoute,
});

function AuthRoute() {
  const search = Route.useSearch();
  return <AuthPage isRegister={search.mode === "register"} />;
}

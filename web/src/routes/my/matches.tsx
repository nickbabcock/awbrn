import { createFileRoute, redirect } from "@tanstack/react-router";
import { ensureSessionFn } from "#/auth/auth.functions.ts";
import { MyMatchesPage } from "#/matches/screens/MyMatchesPage.tsx";
import { listMyMatchesFn } from "#/matches/matches.functions.ts";

export const Route = createFileRoute("/my/matches")({
  loader: async () => {
    try {
      await ensureSessionFn();
      const data = await listMyMatchesFn();
      return {
        ...data,
        loadedAt: new Date().toISOString(),
      };
    } catch (error) {
      if (isUnauthorizedResponse(error)) {
        throw redirect({ to: "/auth" });
      }
      throw error;
    }
  },
  component: MyMatchesRoute,
});

function MyMatchesRoute() {
  const data = Route.useLoaderData();
  return <MyMatchesPage loadedAt={data.loadedAt} matches={data.matches} />;
}

function isUnauthorizedResponse(error: unknown): boolean {
  return (
    (error instanceof Response && error.status === 401) ||
    (typeof error === "object" && error !== null && "status" in error && error.status === 401)
  );
}

import { createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { MyMatchesPage } from "#/matches/screens/MyMatchesPage.tsx";

export const Route = createFileRoute("/my/matches")({
  loader: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    if (!session) {
      throw redirect({ to: "/auth" });
    }

    await context.queryClient.ensureQueryData(myMatchesQueryOptions());
  },
  component: MyMatchesRoute,
});

function MyMatchesRoute() {
  return <MyMatchesPage />;
}

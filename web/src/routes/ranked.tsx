import { createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { RankedHubPage } from "#/matchmaking/screens/RankedHubPage.tsx";
import {
  rankedOverviewQueryOptions,
  rankedStandingsQueryOptions,
} from "#/matchmaking/matchmaking.queries.ts";
import { rankedPoolSchema, type RankedPool } from "#/matches/schemas.ts";

/** The pool the hub opens on, and the one the address leaves out. */
const DEFAULT_RANKED_POOL: RankedPool = "async";

type RankedSearch = {
  /** Absent for the default pool, which keeps the plain address clean. */
  pool?: RankedPool;
};

export const Route = createFileRoute("/ranked")({
  // The pool lives in the address, so a player can keep one open in a tab. An
  // address which names no pool, or a pool which does not exist, opens the
  // default rather than failing.
  validateSearch: (search: Record<string, unknown>): RankedSearch => {
    const chosen = rankedPoolSchema.safeParse(search.pool);
    return chosen.success && chosen.data !== DEFAULT_RANKED_POOL ? { pool: chosen.data } : {};
  },
  loaderDeps: ({ search }) => ({ pool: search.pool ?? DEFAULT_RANKED_POOL }),
  loader: async ({ context, deps }) => {
    const session = await context.queryClient.ensureQueryData(sessionQueryOptions());
    if (!session) {
      throw redirect({ to: "/auth" });
    }

    // The standings are part of the first view, so they render with it rather
    // than arriving after it and pushing the panel down.
    await Promise.all([
      context.queryClient.ensureQueryData(rankedOverviewQueryOptions()),
      context.queryClient.ensureQueryData(rankedStandingsQueryOptions(deps.pool)),
    ]);
  },
  component: RankedRoute,
});

function RankedRoute() {
  const { pool = DEFAULT_RANKED_POOL } = Route.useSearch();
  const navigate = Route.useNavigate();

  return (
    <RankedHubPage
      onSelectPool={(next) =>
        void navigate({ search: next === DEFAULT_RANKED_POOL ? {} : { pool: next } })
      }
      pool={pool}
    />
  );
}

import { useSession } from "./auth-client";
import type { Session } from "../server/auth";
import { Route as RootRoute } from "../routes/__root";

export function useAppSession(): Session | null {
  const serverSession = RootRoute.useLoaderData();
  const { data: clientSession } = useSession();
  return clientSession ?? serverSession;
}

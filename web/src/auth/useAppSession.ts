import type { Session } from "./session";
import { Route as RootRoute } from "../routes/__root";

export function useAppSession(): Session | null {
  return RootRoute.useLoaderData();
}

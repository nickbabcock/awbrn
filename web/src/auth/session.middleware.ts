import { createMiddleware } from "@tanstack/react-start";
import { getRequest } from "@tanstack/react-start/server";
import { getRequestSession } from "./auth.server";

export const sessionMiddleware = createMiddleware().server(async ({ next }) => {
  const session = await getRequestSession(getRequest());
  return next({ context: { session } });
});

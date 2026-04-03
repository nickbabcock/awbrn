import { createMiddleware } from "@tanstack/react-start";
import { getRequest } from "@tanstack/react-start/server";
import { getRequestSession } from "./server/auth";

export const sessionMiddleware = createMiddleware().server(async ({ next }) => {
  const session = await getRequestSession(getRequest());
  return next({ context: { session } });
});

import { createServerFn } from "@tanstack/react-start";
import { getRequest } from "@tanstack/react-start/server";
import { getRequestSession, requireRequestSession } from "./auth.server";

export const getSessionFn = createServerFn({ method: "GET" }).handler(() => {
  return getRequestSession(getRequest());
});

export const ensureSessionFn = createServerFn({ method: "GET" }).handler(async () => {
  return requireRequestSession(getRequest());
});

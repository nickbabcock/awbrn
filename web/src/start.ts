import { createStart } from "@tanstack/react-start";
import { createMiddleware } from "@tanstack/react-start";
import { getResponseHeaders, setResponseHeaders } from "@tanstack/react-start/server";

const crossOriginIsolationMiddleware = createMiddleware().server(async ({ next }) => {
  const responseHeaders = getResponseHeaders();
  const headersToSet: Record<string, string> = {};

  if (responseHeaders.get("Cross-Origin-Embedder-Policy") !== "require-corp") {
    headersToSet["Cross-Origin-Embedder-Policy"] = "require-corp";
  }

  if (responseHeaders.get("Cross-Origin-Opener-Policy") !== "same-origin") {
    headersToSet["Cross-Origin-Opener-Policy"] = "same-origin";
  }

  if (Object.keys(headersToSet).length > 0) {
    setResponseHeaders(headersToSet);
  }

  return next();
});

export const startInstance = createStart(() => ({
  requestMiddleware: [crossOriginIsolationMiddleware],
}));

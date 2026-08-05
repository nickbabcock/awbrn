import { createStart } from "@tanstack/react-start";
import { createMiddleware } from "@tanstack/react-start";
import { getResponseHeaders, setResponseHeader } from "@tanstack/react-start/server";

const crossOriginIsolationMiddleware = createMiddleware().server(async ({ next }) => {
  const responseHeaders = getResponseHeaders();

  if (responseHeaders.get("Cross-Origin-Embedder-Policy") !== "require-corp") {
    setResponseHeader("Cross-Origin-Embedder-Policy", "require-corp");
  }

  if (responseHeaders.get("Cross-Origin-Opener-Policy") !== "same-origin") {
    setResponseHeader("Cross-Origin-Opener-Policy", "same-origin");
  }

  return next();
});

export const startInstance = createStart(() => ({
  requestMiddleware: [crossOriginIsolationMiddleware],
}));

import { createFileRoute } from "@tanstack/react-router";
import { getAuth } from "#/auth/auth.server.ts";

export const Route = createFileRoute("/api/auth/$")({
  server: {
    handlers: {
      GET: ({ request }) => getAuth().handler(request),
      POST: ({ request }) => getAuth().handler(request),
    },
  },
});

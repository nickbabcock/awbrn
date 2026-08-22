import { createServerFn } from "@tanstack/react-start";
import { mapRefSchema } from "./schemas.ts";
import { loadMapRevision } from "./maps.server.ts";

export const getMapRevisionFn = createServerFn({ method: "GET" })
  .validator(mapRefSchema)
  .handler(async ({ data }) => loadMapRevision(data));

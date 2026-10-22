import { createServerFn } from "@tanstack/react-start";
import { getRequest } from "@tanstack/react-start/server";
import { z } from "zod";
import { readAwbwGame } from "./awbw.server.ts";

/**
 * The armies, map and turn of an AWBW game, which fill the move planner's
 * setup.
 */
export const getAwbwGameFn = createServerFn({ method: "GET" })
  .validator(z.object({ gameId: z.number().int().positive() }))
  .handler(({ data }) => readAwbwGame(getRequest(), data.gameId));

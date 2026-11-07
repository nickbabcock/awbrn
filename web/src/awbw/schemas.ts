import { z } from "zod";

export const predeployedUnitSchema = z.object({
  "Unit ID": z.number().int(),
  "Unit X": z.number().int(),
  "Unit Y": z.number().int(),
  "Unit HP": z.number().int(),
  "Country Code": z.string(),
});

export const awbwMapDataSchema = z.object({
  Name: z.string(),
  Author: z.string(),
  "Player Count": z.number(),
  "Published Date": z.string(),
  "Size X": z.number(),
  "Size Y": z.number(),
  "Terrain Map": z.array(z.array(z.number())),
  "Predeployed Units": z.array(predeployedUnitSchema),
});

export type PredeployedUnit = z.infer<typeof predeployedUnitSchema>;
export type AwbwMapData = z.infer<typeof awbwMapDataSchema>;

/** One army of an AWBW game page, as `game.php` embeds it in `playersInfo`. */
export const awbwGamePagePlayerSchema = z.object({
  players_id: z.number().int(),
  users_username: z.string(),
  players_team: z.string(),
  players_order: z.number().int(),
  players_eliminated: z.enum(["Y", "N"]),
  players_co_id: z.number().int(),
  // AWBW records ten times the charge that AWVM counts.
  players_co_power: z.number().int(),
  players_co_power_on: z.enum(["N", "Y", "S"]),
  // The costs before any power use, and the costs now.
  co_max_power: z.number().int(),
  co_max_spower: z.number().int(),
  players_co_max_power: z.number().int(),
  players_co_max_spower: z.number().int(),
  // "?" when fog hides the funds.
  players_funds: z.union([z.number(), z.string()]),
  countries_code: z.string(),
});

/** What the planner reads from one AWBW game, for one army. */
export interface AwbwGameArmy {
  playerId: number;
  username: string;
  factionCode: string;
  team: string;
  order: number;
  eliminated: boolean;
  coId: number;
  /** The power charge, in the units that AWBW records. */
  coPower: number;
  powerOn: "none" | "cop" | "scop";
  /** The super power cost before any power use, and now. */
  scopBaseCost: number;
  scopCost: number;
  /** The normal power cost before any power use, and now. */
  copBaseCost: number;
  copCost: number;
  /** Null when fog hides the funds. */
  funds: number | null;
}

/**
 * What the planner reads from one AWBW game page.
 *
 * The server makes it from the page, and the planner gets it through a server
 * function, so it has no schema.
 */
export interface AwbwGameSnapshot {
  gameId: number;
  mapId: number | null;
  /** The player whose turn it is. */
  activePlayerId: number | null;
  armies: AwbwGameArmy[];
}

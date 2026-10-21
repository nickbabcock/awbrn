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
export const awbwGameArmySchema = z.object({
  playerId: z.number().int(),
  username: z.string(),
  factionCode: z.string(),
  team: z.string(),
  order: z.number().int(),
  eliminated: z.boolean(),
  coId: z.number().int(),
  /** The power charge, in the units that AWBW records. */
  coPower: z.number().int(),
  powerOn: z.enum(["none", "cop", "scop"]),
  /** The super power cost before any power use, and now. */
  scopBaseCost: z.number().int(),
  scopCost: z.number().int(),
  /** The normal power cost before any power use, and now. */
  copBaseCost: z.number().int(),
  copCost: z.number().int(),
  /** Null when fog hides the funds. */
  funds: z.number().int().nullable(),
});

/** What the planner reads from one AWBW game page. */
export const awbwGameSnapshotSchema = z.object({
  gameId: z.number().int(),
  mapId: z.number().int().nullable(),
  /** The player whose turn it is. */
  activePlayerId: z.number().int().nullable(),
  armies: z.array(awbwGameArmySchema),
});

export type AwbwGameArmy = z.infer<typeof awbwGameArmySchema>;
export type AwbwGameSnapshot = z.infer<typeof awbwGameSnapshotSchema>;

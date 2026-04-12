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

import { z } from "zod";

export const awbwMapDataSchema = z.object({
  Name: z.string(),
  Author: z.string(),
  "Player Count": z.number(),
  "Published Date": z.string(),
  "Size X": z.number(),
  "Size Y": z.number(),
  "Terrain Map": z.array(z.array(z.number())),
  "Predeployed Units": z.array(z.unknown()),
});

export type AwbwMapData = z.infer<typeof awbwMapDataSchema>;

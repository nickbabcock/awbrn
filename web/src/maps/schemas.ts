import { z } from "zod";
import { MAP_ID_LENGTH } from "./map_id.ts";

/** External systems that can provide maps. */
export const MAP_SOURCE_KINDS = ["awbw"] as const;

export const mapSourceKindSchema = z.enum(MAP_SOURCE_KINDS);

export type MapSourceKind = z.infer<typeof mapSourceKindSchema>;

/** Stable map identity plus revision. */
export const mapRefSchema = z.object({
  mapId: z
    .string()
    .length(MAP_ID_LENGTH)
    .regex(/^[0-9a-z]+$/),
  revision: z.number().int().positive(),
});

export type MapRef = z.infer<typeof mapRefSchema>;

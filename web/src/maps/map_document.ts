import { z } from "zod";

export const awbrnMapDocumentSchema = z
  .object({
    map_format: z.literal(1),
    width: z.number().int().positive().max(64),
    height: z.number().int().positive().max(64),
    terrain: z.array(z.number().int().min(1).max(255)),
    units: z.array(
      z.object({
        position: z.tuple([z.number().int().nonnegative(), z.number().int().nonnegative()]),
        unit: z.string(),
        faction: z.string(),
        hp: z.number().int().min(1).max(10),
      }),
    ),
    metadata: z.object({
      name: z.string(),
      author: z.string(),
      player_count: z.number().int().positive(),
    }),
  })
  .superRefine((document, context) => {
    if (document.terrain.length !== document.width * document.height) {
      context.addIssue({ code: "custom", message: "terrain dimensions do not match" });
    }
    for (const unit of document.units) {
      if (unit.position[0] >= document.width || unit.position[1] >= document.height) {
        context.addIssue({ code: "custom", message: "predeployed unit is outside the map" });
      }
    }
  });

export type AwbrnMapDocument = z.infer<typeof awbrnMapDocumentSchema>;

export const importedMapDocumentSchema = z.object({
  document: awbrnMapDocumentSchema,
  contentHash: z.string().regex(/^[0-9a-f]{64}$/),
  propertySignature: z.string().regex(/^[0-9a-f]{64}$/),
  unitSignature: z.string().regex(/^[0-9a-f]{64}$/),
});

export type ImportedMapDocument = z.infer<typeof importedMapDocumentSchema>;

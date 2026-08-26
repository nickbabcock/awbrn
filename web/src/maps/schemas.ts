import { z } from "zod";
import { MAP_SEARCH_MAX_LENGTH } from "./map_catalog.ts";
import { MAP_ID_LENGTH } from "./map_id.ts";

/** External systems that can provide maps. */
export const MAP_SOURCE_KINDS = ["awbw"] as const;

export const mapSourceKindSchema = z.enum(MAP_SOURCE_KINDS);

export type MapSourceKind = z.infer<typeof mapSourceKindSchema>;

export const mapIdSchema = z
  .string()
  .length(MAP_ID_LENGTH)
  .regex(/^[0-9a-z]+$/);

/** Stable map identity plus revision. */
export const mapRefSchema = z.object({
  mapId: mapIdSchema,
  revision: z.number().int().positive(),
});

export type MapRef = z.infer<typeof mapRefSchema>;

/**
 * How good a map revision is, from C at the bottom to S at the top.
 *
 * The order of this list is the order of the ranks. A new revision has no
 * rank: content that changed must earn its rank again.
 */
export const MAP_RANKS = ["C", "B", "A", "S"] as const;

export const mapRankSchema = z.enum(MAP_RANKS);

export type MapRank = z.infer<typeof mapRankSchema>;

/**
 * How a map plays. A map carries as many tags as fit it, or none.
 *
 * Tags belong to the map and not to one of its revisions, because a revision
 * changes the terrain of a map and not the kind of game it makes.
 */
export const MAP_TAGS = ["standard", "fog", "team", "ffa", "high-funds"] as const;

export const mapTagSchema = z.enum(MAP_TAGS);

export type MapTag = z.infer<typeof mapTagSchema>;

/** What each tag is called on screen. */
export const MAP_TAG_LABELS: Record<MapTag, string> = {
  standard: "Standard",
  fog: "Fog",
  team: "Team",
  ffa: "FFA",
  "high-funds": "High funds",
};

/** Give a map revision a rank, or take the rank it holds away. */
export const mapRankUpdateSchema = z.object({
  map: mapRefSchema,
  rank: mapRankSchema.nullable(),
});

export type MapRankUpdate = z.infer<typeof mapRankUpdateSchema>;

/** Replace every tag on a map with the tags named here. */
export const mapTagsUpdateSchema = z.object({
  mapId: mapIdSchema,
  tags: z.array(mapTagSchema).max(MAP_TAGS.length),
});

export type MapTagsUpdate = z.infer<typeof mapTagsUpdateSchema>;

/** What the catalog is asked for: a page of it, and what to look for. */
export const mapCatalogRequestSchema = z.object({
  cursor: z.string().min(1).optional(),
  search: z.string().max(MAP_SEARCH_MAX_LENGTH).optional(),
});

export type MapCatalogRequest = z.infer<typeof mapCatalogRequestSchema>;

/** An AWBW map a player asks AWBRN to hold. */
export const awbwMapImportRequestSchema = z.object({
  sourceMapId: z.number().int().positive(),
});

export type AwbwMapImportRequest = z.infer<typeof awbwMapImportRequestSchema>;

/** Where a map in the catalog came from. */
export interface MapOrigin {
  kind: MapSourceKind;
  sourceMapId: number;
}

/** One map, as the catalog lists it. */
export interface MapCatalogEntry {
  mapId: string;
  revision: number;
  name: string;
  author: string;
  playerCount: number;
  /** The rank of this revision, or null while it is unranked. */
  rank: MapRank | null;
  /** The tags of the map, in vocabulary order. */
  tags: MapTag[];
  width: number;
  height: number;
  origin: MapOrigin | null;
  /** Addresses of the two pictures of this revision. */
  screenshot: { small: string; full: string };
  addedAt: string;
}

export interface MapCatalogResponse {
  maps: MapCatalogEntry[];
  pageSize: number;
  hasNextPage: boolean;
  nextCursor: string | null;
}

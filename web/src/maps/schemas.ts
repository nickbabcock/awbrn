import { z } from "zod";
import { MAP_SEARCH_MAX_LENGTH } from "./map_catalog.ts";
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

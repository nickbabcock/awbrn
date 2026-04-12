import { queryOptions } from "@tanstack/react-query";
import { awbwKeys } from "./awbw.keys";
import { awbwMapAssetPath } from "./paths";
import { awbwMapDataSchema, type AwbwMapData } from "./schemas";

export type AwbwMapDataQueryErrorKind = "notFound" | "loadFailed";

export class AwbwMapDataQueryError extends Error {
  readonly kind: AwbwMapDataQueryErrorKind;

  constructor(kind: AwbwMapDataQueryErrorKind) {
    super(kind === "notFound" ? "Map not found." : "Map metadata could not be loaded.");
    this.name = "AwbwMapDataQueryError";
    this.kind = kind;
  }
}

export function awbwMapDataQueryOptions(mapId: number) {
  return queryOptions({
    queryKey: awbwKeys.mapData(mapId),
    queryFn: ({ signal }) => fetchAwbwMapData(mapId, signal),
    retry: false,
  });
}

async function fetchAwbwMapData(mapId: number, signal: AbortSignal): Promise<AwbwMapData> {
  const response = await fetch(awbwMapAssetPath(mapId), { signal });

  if (response.status === 404) {
    throw new AwbwMapDataQueryError("notFound");
  }

  if (!response.ok) {
    throw new AwbwMapDataQueryError("loadFailed");
  }

  let payload: unknown;

  try {
    payload = await response.json();
  } catch {
    throw new AwbwMapDataQueryError("loadFailed");
  }

  const parsed = awbwMapDataSchema.safeParse(payload);

  if (!parsed.success) {
    throw new AwbwMapDataQueryError("loadFailed");
  }

  return parsed.data;
}

import { waitUntil } from "cloudflare:workers";
import { parseAwbwUsername } from "./parsers";
import { awbwMapDataSchema, type AwbwMapData } from "./schemas";

const AWBW_BASE_URL = "https://awbw.amarriner.com";
const AWBW_FETCH_TIMEOUT_MS = 5000;
const AWBW_EDGE_TTL_SECONDS = 60 * 60 * 24 * 7;
const AWBW_SUCCESS_CACHE_CONTROL = `public, s-maxage=${AWBW_EDGE_TTL_SECONDS}, max-age=0, must-revalidate`;
const AWBW_NO_STORE_CACHE_CONTROL = "no-store";

export async function fetchAwbwUsernameResponse(
  request: Request,
  userId: number,
): Promise<Response> {
  return withCachedResponse(request, () => fetchAwbwUsername(userId));
}

export async function fetchAwbwUsername(userId: number): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => {
    controller.abort();
  }, AWBW_FETCH_TIMEOUT_MS);

  try {
    const response = await fetch(`${AWBW_BASE_URL}/profile.php?users_id=${userId}`, {
      signal: controller.signal,
    });
    if (!response.ok) {
      return Response.json(
        { userId, username: null },
        {
          headers: { "Cache-Control": AWBW_NO_STORE_CACHE_CONTROL },
          status: 502,
        },
      );
    }

    const html = await response.text();
    const username = parseAwbwUsername(html);

    return Response.json(
      { userId, username },
      {
        headers: {
          "Cache-Control":
            username !== null ? AWBW_SUCCESS_CACHE_CONTROL : AWBW_NO_STORE_CACHE_CONTROL,
        },
      },
    );
  } catch {
    return Response.json(
      { userId, username: null },
      {
        headers: { "Cache-Control": AWBW_NO_STORE_CACHE_CONTROL },
        status: 502,
      },
    );
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function fetchAwbwMapResponse(request: Request, mapId: number): Promise<Response> {
  return withCachedResponse(request, () => fetchAwbwMap(mapId));
}

export async function fetchAwbwSmallMapResponse(
  request: Request,
  mapId: number,
): Promise<Response> {
  return withCachedResponse(request, () => fetchAwbwSmallMap(mapId));
}

export async function fetchAwbwMap(mapId: number): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => {
    controller.abort();
  }, AWBW_FETCH_TIMEOUT_MS);

  try {
    const response = await fetch(`${AWBW_BASE_URL}/api/map/map_info.php?maps_id=${mapId}`, {
      signal: controller.signal,
    });
    if (response.status === 404) {
      return createTextResponse("Not Found", { status: 404, cacheable: false });
    }
    if (!response.ok) {
      return createTextResponse("Bad Gateway", { status: 502, cacheable: false });
    }

    const body = await response.text();
    const contentType = response.headers.get("Content-Type") ?? "application/json";
    let payload: unknown = null;

    try {
      payload = JSON.parse(body);
    } catch {}

    return createTextResponse(body, {
      status: response.status,
      cacheable: awbwMapDataSchema.safeParse(payload).success,
      contentType,
    });
  } catch {
    return createTextResponse("Bad Gateway", { status: 502, cacheable: false });
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function fetchAwbwSmallMap(mapId: number): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => {
    controller.abort();
  }, AWBW_FETCH_TIMEOUT_MS);

  try {
    const response = await fetch(`${AWBW_BASE_URL}/smallmaps/${mapId}.png`, {
      signal: controller.signal,
    });
    if (response.status === 404) {
      return createTextResponse("Not Found", { status: 404, cacheable: false });
    }
    if (!response.ok) {
      return createTextResponse("Bad Gateway", { status: 502, cacheable: false });
    }

    return createBinaryResponse(await response.arrayBuffer(), {
      cacheable: true,
      contentType: response.headers.get("Content-Type") ?? "image/png",
      status: response.status,
    });
  } catch {
    return createTextResponse("Bad Gateway", { status: 502, cacheable: false });
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function fetchAwbwMapData(mapId: number): Promise<AwbwMapData> {
  const response = await fetchAwbwMap(mapId);

  if (response.status === 404) {
    throw new Error("Map not found");
  }

  if (!response.ok) {
    throw new Error("Failed to fetch map");
  }

  const parsed = awbwMapDataSchema.safeParse(await response.json());

  if (!parsed.success) {
    throw new Error("Map payload was invalid");
  }

  return parsed.data;
}

interface ResponseOptions {
  cacheable: boolean;
  contentType?: string;
  status?: number;
}

function createTextResponse(
  body: string,
  { cacheable, contentType, status = 200 }: ResponseOptions,
): Response {
  const headers = new Headers({
    "Cache-Control": cacheable ? AWBW_SUCCESS_CACHE_CONTROL : AWBW_NO_STORE_CACHE_CONTROL,
  });

  if (contentType) {
    headers.set("Content-Type", contentType);
  }

  return new Response(body, { headers, status });
}

function createBinaryResponse(
  body: ArrayBuffer,
  { cacheable, contentType, status = 200 }: ResponseOptions,
): Response {
  const headers = new Headers({
    "Cache-Control": cacheable ? AWBW_SUCCESS_CACHE_CONTROL : AWBW_NO_STORE_CACHE_CONTROL,
  });

  if (contentType) {
    headers.set("Content-Type", contentType);
  }

  return new Response(body, { headers, status });
}

async function matchCachedResponse(request?: Request): Promise<Response | null> {
  const cache = getEdgeCache();
  if (!cache || !request) {
    return null;
  }

  return (await cache.match(request)) ?? null;
}

async function withCachedResponse(
  request: Request,
  produceResponse: () => Promise<Response>,
): Promise<Response> {
  const cachedResponse = await matchCachedResponse(request);
  if (cachedResponse) {
    console.log(`[awbw-cache] hit ${request.method} ${request.url}`);
    return cachedResponse;
  }

  console.log(`[awbw-cache] miss ${request.method} ${request.url}; fetching upstream`);
  return maybeCacheResponse(request, await produceResponse());
}

function maybeCacheResponse(request: Request, response: Response): Response {
  const cache = getEdgeCache();
  if (!cache || response.headers.get("Cache-Control") !== AWBW_SUCCESS_CACHE_CONTROL) {
    return response;
  }

  waitUntil(cache.put(request, response.clone()));
  return response;
}

function getEdgeCache(): Cache | null {
  return (globalThis.caches as (CacheStorage & { default?: Cache }) | undefined)?.default ?? null;
}

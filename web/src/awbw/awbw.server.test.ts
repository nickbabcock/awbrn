import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const waitUntilMock = vi.hoisted(() => vi.fn());

vi.mock("cloudflare:workers", () => ({
  waitUntil: waitUntilMock,
}));

import { fetchAwbwMapResponse, fetchAwbwUsernameResponse } from "./awbw.server.ts";

const SUCCESS_CACHE_CONTROL = "public, s-maxage=604800, max-age=0, must-revalidate";

function createValidMapPayload() {
  return {
    Name: "Foreign Invasion",
    Author: "AzarelAlt",
    "Player Count": 5,
    "Published Date": "2024-07-31 15:29:14",
    "Size X": 27,
    "Size Y": 21,
    "Terrain Map": [
      [1, 2],
      [3, 4],
    ],
    "Predeployed Units": [],
  };
}

describe("awbw edge caching", () => {
  const fetchMock = vi.fn<typeof fetch>();
  const cacheMatchMock = vi.fn();
  const cachePutMock = vi.fn();

  beforeEach(() => {
    waitUntilMock.mockReset();
    fetchMock.mockReset();
    cacheMatchMock.mockReset();
    cachePutMock.mockReset();

    cacheMatchMock.mockResolvedValue(null);
    cachePutMock.mockResolvedValue(undefined);

    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("caches", {
      default: {
        match: cacheMatchMock,
        put: cachePutMock,
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns cached username responses without hitting AWBW", async () => {
    cacheMatchMock.mockResolvedValue(
      Response.json(
        { userId: 7, username: "Andy" },
        { headers: { "Cache-Control": SUCCESS_CACHE_CONTROL } },
      ),
    );

    const response = await fetchAwbwUsernameResponse(
      new Request("https://example.com/api/awbw/user/7"),
      7,
    );

    expect(fetchMock).not.toHaveBeenCalled();
    expect(cachePutMock).not.toHaveBeenCalled();
    expect(await response.json()).toEqual({ userId: 7, username: "Andy" });
  });

  it("caches successful username lookups for one week", async () => {
    fetchMock.mockResolvedValue(
      new Response("<div>Username: <i>Andy &amp; Max</i></div>", {
        headers: { "Content-Type": "text/html; charset=UTF-8" },
      }),
    );

    const request = new Request("https://example.com/api/awbw/user/7");
    const response = await fetchAwbwUsernameResponse(request, 7);

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(response.headers.get("Cache-Control")).toBe(SUCCESS_CACHE_CONTROL);
    expect(await response.json()).toEqual({ userId: 7, username: "Andy & Max" });
    expect(cachePutMock).toHaveBeenCalledOnce();
    expect(cachePutMock).toHaveBeenCalledWith(request, expect.any(Response));
    expect(waitUntilMock).toHaveBeenCalledOnce();
  });

  it("does not cache username parse failures", async () => {
    fetchMock.mockResolvedValue(new Response("<div>No username here</div>"));

    const response = await fetchAwbwUsernameResponse(
      new Request("https://example.com/api/awbw/user/7"),
      7,
    );

    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(await response.json()).toEqual({ userId: 7, username: null });
    expect(cachePutMock).not.toHaveBeenCalled();
    expect(waitUntilMock).not.toHaveBeenCalled();
  });

  it("returns cached map responses without hitting AWBW", async () => {
    const payload = createValidMapPayload();
    cacheMatchMock.mockResolvedValue(
      new Response(JSON.stringify(payload), {
        headers: {
          "Cache-Control": SUCCESS_CACHE_CONTROL,
          "Content-Type": "application/json",
        },
      }),
    );

    const response = await fetchAwbwMapResponse(
      new Request("https://example.com/api/awbw/map/162795.json"),
      162795,
    );

    expect(fetchMock).not.toHaveBeenCalled();
    expect(cachePutMock).not.toHaveBeenCalled();
    expect(await response.json()).toEqual(payload);
  });

  it("caches valid map payloads for one week", async () => {
    const payload = createValidMapPayload();
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(payload), {
        headers: { "Content-Type": "application/json" },
      }),
    );

    const request = new Request("https://example.com/api/awbw/map/162795.json");
    const response = await fetchAwbwMapResponse(request, 162795);

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(response.headers.get("Cache-Control")).toBe(SUCCESS_CACHE_CONTROL);
    expect(response.headers.get("Content-Type")).toBe("application/json");
    expect(await response.json()).toEqual(payload);
    expect(cachePutMock).toHaveBeenCalledOnce();
    expect(cachePutMock).toHaveBeenCalledWith(request, expect.any(Response));
    expect(waitUntilMock).toHaveBeenCalledOnce();
  });

  it("does not cache AWBW map error payloads", async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify({ err: true, message: "No map matches given ID" }), {
        headers: { "Content-Type": "application/json" },
      }),
    );

    const response = await fetchAwbwMapResponse(
      new Request("https://example.com/api/awbw/map/999999999.json"),
      999999999,
    );

    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(await response.json()).toEqual({ err: true, message: "No map matches given ID" });
    expect(cachePutMock).not.toHaveBeenCalled();
    expect(waitUntilMock).not.toHaveBeenCalled();
  });

  it("does not cache upstream failures", async () => {
    fetchMock.mockResolvedValue(new Response("boom", { status: 503 }));

    const response = await fetchAwbwMapResponse(
      new Request("https://example.com/api/awbw/map/162795.json"),
      162795,
    );

    expect(response.status).toBe(502);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(cachePutMock).not.toHaveBeenCalled();
    expect(waitUntilMock).not.toHaveBeenCalled();
  });

  it("still works when Cache API is unavailable", async () => {
    vi.stubGlobal("caches", undefined);
    fetchMock.mockResolvedValue(new Response("<div>Username: <i>Andy</i></div>"));

    const response = await fetchAwbwUsernameResponse(
      new Request("https://example.com/api/awbw/user/7"),
      7,
    );

    expect(await response.json()).toEqual({ userId: 7, username: "Andy" });
    expect(response.headers.get("Cache-Control")).toBe(SUCCESS_CACHE_CONTROL);
    expect(cachePutMock).not.toHaveBeenCalled();
    expect(waitUntilMock).not.toHaveBeenCalled();
  });
});

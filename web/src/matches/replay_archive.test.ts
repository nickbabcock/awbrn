import { describe, expect, it, vi } from "vitest";
import { getMatchReplayResponse, matchReplayKey, uploadMatchReplay } from "./replay_archive";
import type { MatchSetup } from "./schemas";

const setup = {
  matchId: "abc123def456g",
  mapId: "000000000001",
  revision: 1,
  map: {
    map_format: 1,
    width: 1,
    height: 1,
    terrain: [1],
    units: [],
    metadata: { name: "Test Map", author: "Test Author", player_count: 2 },
  },
  players: [],
  fogEnabled: false,
  startingFunds: 0,
  creatorUserId: "user-1",
} satisfies MatchSetup;

describe("match replay archive", () => {
  it("uploads a versioned replay under its public path", async () => {
    const put = vi.fn().mockResolvedValue(undefined);

    await uploadMatchReplay({ put } as unknown as R2Bucket, setup, [{ command: "endTurn" }]);

    expect(put).toHaveBeenCalledWith(
      "replays/abc123def456g.json",
      JSON.stringify({ version: 1, setup, actions: [{ command: "endTurn" }] }),
      {
        httpMetadata: {
          contentType: "application/json; charset=utf-8",
          cacheControl: "public, max-age=31536000, immutable",
        },
      },
    );
  });

  it("serves an archived replay with public cache metadata", async () => {
    const body = JSON.stringify({ version: 1, setup, actions: [] });
    const get = vi.fn().mockResolvedValue({
      body,
      httpEtag: '"etag"',
      writeHttpMetadata(headers: Headers) {
        headers.set("Content-Type", "application/json; charset=utf-8");
      },
    });

    const response = await getMatchReplayResponse({ get } as unknown as R2Bucket, setup.matchId);

    expect(matchReplayKey(setup.matchId)).toBe("replays/abc123def456g.json");
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("application/json; charset=utf-8");
    expect(response.headers.get("Cache-Control")).toBe("public, max-age=31536000, immutable");
    expect(response.headers.get("ETag")).toBe('"etag"');
    expect(await response.text()).toBe(body);
  });

  it("returns not found when no replay was uploaded", async () => {
    const get = vi.fn().mockResolvedValue(null);

    const response = await getMatchReplayResponse({ get } as unknown as R2Bucket, setup.matchId);

    expect(response.status).toBe(404);
  });
});

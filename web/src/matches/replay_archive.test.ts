import { describe, expect, it, vi } from "vitest";
import {
  getMatchReplayResponse,
  isMatchReplayDownload,
  matchReplayFileName,
  matchReplayKey,
  matchReplayDownloadPath,
  matchReplayPath,
  matchReplaysExist,
  parseMatchReplay,
  uploadMatchReplay,
} from "./replay_archive";
import { defaultMatchClock, type MatchSetup } from "./schemas";

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
  clock: defaultMatchClock,
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

describe("parseMatchReplay", () => {
  it("accepts an archive the uploader wrote", () => {
    const replay = parseMatchReplay({ version: 1, setup, actions: [{ command: "endTurn" }] });

    expect(replay.version).toBe(1);
    expect(replay.setup.matchId).toBe(setup.matchId);
    expect(replay.actions).toHaveLength(1);
  });

  it("rejects a file that is not an AWBRN archive", () => {
    expect(() => parseMatchReplay({ version: 2, setup, actions: [] })).toThrow(
      /not a valid AWBRN archive/,
    );
    expect(() => parseMatchReplay({ version: 1, actions: [] })).toThrow(
      /not a valid AWBRN archive/,
    );
    expect(() => parseMatchReplay("nope")).toThrow(/not a valid AWBRN archive/);
  });
});

describe("matchReplayPath", () => {
  it("points at the route that serves the archive", () => {
    expect(matchReplayPath("abc123def456g")).toBe("/replays/abc123def456g.json");
  });

  it("names the downloaded file after its match", () => {
    expect(matchReplayFileName("abc123def456g")).toBe("awbrn-replay-abc123def456g.json");
  });

  it("asks for the file to keep on its own path, so the two do not share a cache entry", () => {
    expect(matchReplayDownloadPath("abc123def456g")).toBe("/replays/abc123def456g.json?download=1");
    expect(isMatchReplayDownload(new URL("https://awbrn.test/replays/x.json?download=1"))).toBe(
      true,
    );
    expect(isMatchReplayDownload(new URL("https://awbrn.test/replays/x.json"))).toBe(false);
  });
});

describe("matchReplaysExist", () => {
  it("reports only the matches that have a stored archive", async () => {
    const head = vi.fn(async (key: string) => (key.includes("kept") ? {} : null));

    const stored = await matchReplaysExist({ head } as unknown as R2Bucket, ["kept", "missing"]);

    expect(stored).toEqual(new Set(["kept"]));
    expect(head).toHaveBeenCalledWith("replays/kept.json");
    expect(head).toHaveBeenCalledWith("replays/missing.json");
  });

  it("checks nothing when no match is named", async () => {
    const head = vi.fn();
    expect(await matchReplaysExist({ head } as unknown as R2Bucket, [])).toEqual(new Set());
    expect(head).not.toHaveBeenCalled();
  });
});

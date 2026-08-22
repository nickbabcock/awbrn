import type { MatchSetup } from "./schemas";

const REPLAY_CACHE_CONTROL = "public, max-age=31536000, immutable";

export interface MatchReplay {
  version: 1;
  setup: MatchSetup;
  actions: unknown[];
}

export function matchReplayKey(matchId: string): string {
  return `replays/${matchId}.json`;
}

export async function uploadMatchReplay(
  bucket: R2Bucket,
  setup: MatchSetup,
  actions: unknown[],
): Promise<void> {
  const replay: MatchReplay = { version: 1, setup, actions };
  await bucket.put(matchReplayKey(setup.matchId), JSON.stringify(replay), {
    httpMetadata: {
      contentType: "application/json; charset=utf-8",
      cacheControl: REPLAY_CACHE_CONTROL,
    },
  });
}

export async function getMatchReplayResponse(bucket: R2Bucket, matchId: string): Promise<Response> {
  const replay = await bucket.get(matchReplayKey(matchId));
  if (replay === null) {
    return new Response("Replay not found", { status: 404 });
  }

  const headers = new Headers();
  replay.writeHttpMetadata(headers);
  headers.set("Cache-Control", REPLAY_CACHE_CONTROL);
  headers.set("ETag", replay.httpEtag);
  return new Response(replay.body, { headers });
}

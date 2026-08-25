import { z } from "zod";
import { matchSetupSchema, type MatchSetup } from "./schemas";

const REPLAY_CACHE_CONTROL = "public, max-age=31536000, immutable";
/** Marks a request for the archive as a file rather than as a document. */
const REPLAY_DOWNLOAD_PARAM = "download";

/**
 * A stored match archive: the setup the match started from, and every action
 * that was taken in it.
 *
 * The actions stay unvalidated here. They are the engine's own action events,
 * and the engine is the only thing that can read them; a schema restated on
 * this side would be a second, weaker copy of the rules the spec already owns.
 */
export const matchReplaySchema = z.object({
  version: z.literal(1),
  setup: matchSetupSchema,
  actions: z.array(z.unknown()),
});

export type MatchReplay = z.infer<typeof matchReplaySchema>;

export function matchReplayKey(matchId: string): string {
  return `replays/${matchId}.json`;
}

/** The path the archive is served from, for reading it in the page. */
export function matchReplayPath(matchId: string): string {
  return `/replays/${matchId}.json`;
}

/**
 * The same archive, asked for as a file to keep.
 *
 * The query is what separates the two: the plain path stays a cacheable
 * document the page can read, and this one carries the disposition that names
 * the saved file.
 */
export function matchReplayDownloadPath(matchId: string): string {
  return `${matchReplayPath(matchId)}?${REPLAY_DOWNLOAD_PARAM}=1`;
}

/** Whether a request asked for the archive as a file to keep. */
export function isMatchReplayDownload(url: URL): boolean {
  return url.searchParams.get(REPLAY_DOWNLOAD_PARAM) === "1";
}

/** The name a downloaded archive is saved under. */
export function matchReplayFileName(matchId: string): string {
  return `awbrn-replay-${matchId}.json`;
}

/**
 * Read a served archive and check it.
 *
 * This is the step playback starts from: it turns the file into a `MatchSetup`
 * the engine can be started with and the action log to feed it, both measured
 * against the schemas the server wrote them with. Nothing plays a stored
 * archive back yet, so today only the download uses the file.
 */
export async function fetchMatchReplay(
  matchId: string,
  fetchImpl: typeof fetch = fetch,
): Promise<MatchReplay> {
  const response = await fetchImpl(matchReplayPath(matchId));
  if (!response.ok) {
    throw new Error(
      response.status === 404
        ? "this match has no stored replay"
        : `the replay could not be read (${response.status})`,
    );
  }

  return parseMatchReplay(await response.json());
}

/** Check a decoded archive. Throws with the first failing field. */
export function parseMatchReplay(value: unknown): MatchReplay {
  const result = matchReplaySchema.safeParse(value);
  if (!result.success) {
    const issue = result.error.issues[0];
    const path = issue?.path.join(".");
    throw new Error(`the replay file is not a valid AWBRN archive${path ? ` (${path})` : ""}`);
  }

  return result.data;
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

/** Whether an archive was stored for each of these matches. */
export async function matchReplaysExist(
  bucket: R2Bucket,
  matchIds: readonly string[],
): Promise<Set<string>> {
  const found = await Promise.all(
    matchIds.map(async (matchId) => {
      const head = await bucket.head(matchReplayKey(matchId));
      return head === null ? null : matchId;
    }),
  );

  return new Set(found.filter((matchId): matchId is string => matchId !== null));
}

export async function getMatchReplayResponse(
  bucket: R2Bucket,
  matchId: string,
  { asAttachment = false }: { asAttachment?: boolean } = {},
): Promise<Response> {
  const replay = await bucket.get(matchReplayKey(matchId));
  if (replay === null) {
    return new Response("Replay not found", { status: 404 });
  }

  const headers = new Headers();
  replay.writeHttpMetadata(headers);
  headers.set("Cache-Control", REPLAY_CACHE_CONTROL);
  headers.set("ETag", replay.httpEtag);
  if (asAttachment) {
    headers.set("Content-Disposition", `attachment; filename="${matchReplayFileName(matchId)}"`);
  }
  return new Response(replay.body, { headers });
}

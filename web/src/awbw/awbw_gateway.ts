import { DurableObject } from "cloudflare:workers";

const AWBW_BASE_URL = "https://awbw.amarriner.com";
const FETCH_TIMEOUT_MS = 5_000;
const MAX_CONCURRENT_REQUESTS = 2;
const MINIMUM_START_INTERVAL_MS = 300;
const MAX_QUEUED_REQUESTS = 50;

interface BufferedResponse {
  body: ArrayBuffer;
  headers: [string, string][];
  status: number;
}

interface PendingRequest {
  key: string;
  reject: (error: unknown) => void;
  removeAbortListener: () => void;
  resolve: (response: BufferedResponse) => void;
  result: Promise<BufferedResponse>;
  signal: AbortSignal;
  target: URL;
}

export class AwbwGatewayDurableObject extends DurableObject<CloudflareBindings> {
  private activeRequests = 0;
  private readonly inFlight = new Map<string, Promise<BufferedResponse>>();
  private nextStartAt = 0;
  private readonly pending: PendingRequest[] = [];
  private pumpTimer: ReturnType<typeof setTimeout> | null = null;

  async fetch(request: Request): Promise<Response> {
    const target = resolveAwbwTarget(new URL(request.url));
    if (!target) return new Response("Not Found", { status: 404 });

    const key = target.toString();
    const existing = this.inFlight.get(key);
    if (existing) return createResponse(await existing);

    if (request.signal.aborted) throw abortReason(request.signal);

    if (this.pending.length >= MAX_QUEUED_REQUESTS) {
      return new Response("AWBW gateway is busy", {
        status: 503,
        headers: { "Retry-After": "1" },
      });
    }

    let rejectResult!: (error: unknown) => void;
    let resolveResult!: (response: BufferedResponse) => void;
    const result = new Promise<BufferedResponse>((resolve, reject) => {
      rejectResult = reject;
      resolveResult = resolve;
    });
    const pendingRequest: PendingRequest = {
      key,
      reject: rejectResult,
      removeAbortListener: () => request.signal.removeEventListener("abort", abortQueued),
      resolve: resolveResult,
      result,
      signal: request.signal,
      target,
    };
    const abortQueued = () => {
      const index = this.pending.indexOf(pendingRequest);
      if (index < 0) return;

      this.pending.splice(index, 1);
      this.cancelPending(pendingRequest);
      this.pump();
    };
    request.signal.addEventListener("abort", abortQueued, { once: true });
    this.pending.push(pendingRequest);
    this.inFlight.set(key, result);
    const removeInFlight = () => {
      if (this.inFlight.get(key) === result) this.inFlight.delete(key);
    };
    void result.then(removeInFlight, removeInFlight);
    this.pump();
    return createResponse(await result);
  }

  private pump(): void {
    if (this.pumpTimer !== null || this.activeRequests >= MAX_CONCURRENT_REQUESTS) return;

    const next = this.pending.shift();
    if (!next) return;

    if (next.signal.aborted) {
      this.cancelPending(next);
      this.pump();
      return;
    }

    const delay = Math.max(0, this.nextStartAt - Date.now());
    if (delay > 0) {
      this.pending.unshift(next);
      this.pumpTimer = setTimeout(() => {
        this.pumpTimer = null;
        this.pump();
      }, delay);
      return;
    }

    next.removeAbortListener();
    this.activeRequests += 1;
    this.nextStartAt = Date.now() + MINIMUM_START_INTERVAL_MS;
    void this.run(next);
    this.pump();
  }

  private async run(request: PendingRequest): Promise<void> {
    try {
      request.resolve(await fetchBuffered(request.target));
    } catch (error) {
      request.reject(error);
    } finally {
      this.activeRequests -= 1;
      this.pump();
    }
  }

  private cancelPending(request: PendingRequest): void {
    request.removeAbortListener();
    if (this.inFlight.get(request.key) === request.result) {
      this.inFlight.delete(request.key);
    }
    request.reject(abortReason(request.signal));
  }
}

function abortReason(signal: AbortSignal): unknown {
  return signal.reason ?? new DOMException("The request was aborted", "AbortError");
}

function resolveAwbwTarget(requestUrl: URL): URL | null {
  const match = /^\/(map|smallmap|user)\/(\d+)$/.exec(requestUrl.pathname);
  if (!match) return null;

  const [, kind, id] = match;
  switch (kind) {
    case "map":
      return new URL(`/api/map/map_info.php?maps_id=${id}`, AWBW_BASE_URL);
    case "smallmap":
      return new URL(`/smallmaps/${id}.png`, AWBW_BASE_URL);
    case "user":
      return new URL(`/profile.php?users_id=${id}`, AWBW_BASE_URL);
    default:
      return null;
  }
}

async function fetchBuffered(target: URL): Promise<BufferedResponse> {
  const response = await fetch(target, { signal: AbortSignal.timeout(FETCH_TIMEOUT_MS) });
  return {
    body: await response.arrayBuffer(),
    headers: [...response.headers.entries()],
    status: response.status,
  };
}

function createResponse(response: BufferedResponse): Response {
  return new Response(response.body.slice(0), {
    headers: response.headers,
    status: response.status,
  });
}

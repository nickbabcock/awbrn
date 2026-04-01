export interface MatchError {
  code: string;
  message: string;
  httpStatus: number;
  details?: unknown;
}

export interface MatchSuccess<T> {
  ok: true;
  value: T;
}

export interface MatchFailure {
  ok: false;
  error: MatchError;
}

export type MatchResult<T> = MatchSuccess<T> | MatchFailure;

export interface MatchCreateResponse {
  matchId: string;
}

const WASM_ERROR_PREFIX = "AWBRN_MATCH_ERROR:";

export function ok<T>(value: T): MatchSuccess<T> {
  return { ok: true, value };
}

export function err(
  code: string,
  message: string,
  httpStatus: number,
  details?: unknown,
): MatchFailure {
  return {
    ok: false,
    error: {
      code,
      message,
      httpStatus,
      details,
    },
  };
}

export function responseFromResult<T>(result: MatchResult<T>, successStatus = 200): Response {
  if (!result.ok) {
    return Response.json({ error: result.error }, { status: result.error.httpStatus });
  }

  return Response.json(result.value, { status: successStatus });
}

export async function parseJsonBody(request: Request): Promise<MatchResult<unknown>> {
  try {
    return ok(await request.json());
  } catch (error) {
    return err("invalidJson", "request body must be valid JSON", 400, {
      reason: error instanceof Error ? error.message : String(error),
    });
  }
}

export function normalizeCaughtError(error: unknown): MatchFailure {
  const wasmError = parseWasmMatchError(error);
  if (wasmError) {
    return {
      ok: false,
      error: wasmError,
    };
  }

  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string" &&
    "message" in error &&
    typeof error.message === "string" &&
    "httpStatus" in error &&
    typeof error.httpStatus === "number"
  ) {
    return {
      ok: false,
      error: {
        code: error.code,
        message: error.message,
        httpStatus: error.httpStatus,
        details: "details" in error ? error.details : undefined,
      },
    };
  }

  return err("internalError", "unexpected match failure", 500, {
    reason: error instanceof Error ? error.message : String(error),
  });
}

function parseWasmMatchError(error: unknown): MatchError | null {
  if (!(error instanceof Error) || !error.message.startsWith(WASM_ERROR_PREFIX)) {
    return null;
  }

  try {
    const parsed = JSON.parse(error.message.slice(WASM_ERROR_PREFIX.length)) as {
      code?: unknown;
      message?: unknown;
      httpStatus?: unknown;
      details?: unknown;
    };

    if (
      typeof parsed.code === "string" &&
      typeof parsed.message === "string" &&
      typeof parsed.httpStatus === "number"
    ) {
      return {
        code: parsed.code,
        message: parsed.message,
        httpStatus: parsed.httpStatus,
        details: parsed.details,
      };
    }
  } catch {
    return null;
  }

  return null;
}

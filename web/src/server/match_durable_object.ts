import { DurableObject } from "cloudflare:workers";
import { WasmMatch, initSync } from "../wasm/awbrn_server";
import matchWasmModule from "../wasm/awbrn_server_bg.wasm";
import {
  err,
  normalizeCaughtError,
  ok,
  type MatchCreateResponse,
  type MatchResult,
} from "./match_protocol";

const CREATE_EVENTS_TABLE_SQL = `
  CREATE TABLE IF NOT EXISTS events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
  )
`;

let wasmInitialized = false;

function ensureMatchWasmInitialized(): void {
  if (wasmInitialized) {
    return;
  }

  initSync({ module: matchWasmModule });
  wasmInitialized = true;
}

function matchError(
  code: string,
  message: string,
  httpStatus: number,
  details?: unknown,
): MatchResult<never> {
  return err(code, message, httpStatus, details);
}

export class MatchDurableObject extends DurableObject<CloudflareBindings> {
  private readonly state: DurableObjectState;
  private readonly schemaReady: Promise<void>;

  constructor(ctx: DurableObjectState, env: CloudflareBindings) {
    super(ctx, env);
    this.state = ctx;
    this.schemaReady = ctx.blockConcurrencyWhile(async () => {
      ctx.storage.sql.exec(CREATE_EVENTS_TABLE_SQL);
    });
  }

  async initializeMatch(setup: unknown): Promise<MatchResult<MatchCreateResponse>> {
    await this.schemaReady;

    try {
      if (this.hasPersistedEvents()) {
        return matchError(
          "matchAlreadyInitialized",
          "match durable object has already been initialized",
          409,
          { matchId: this.state.id.toString() },
        );
      }

      ensureMatchWasmInitialized();
      new WasmMatch(setup);
      this.appendEvent("setup", setup);

      return ok({ matchId: this.state.id.toString() });
    } catch (error) {
      return normalizeCaughtError(error);
    }
  }

  private hasPersistedEvents(): boolean {
    const row = this.state.storage.sql.exec("SELECT COUNT(*) AS event_count FROM events").one() as {
      event_count?: number;
    } | null;
    return (row?.event_count ?? 0) > 0;
  }

  private appendEvent(kind: string, payload: unknown): void {
    this.state.storage.sql.exec(
      "INSERT INTO events (kind, payload_json, created_at_ms) VALUES (?, ?, ?)",
      kind,
      JSON.stringify(payload),
      Date.now(),
    );
  }
}

import { DurableObject } from "cloudflare:workers";
import { drizzle, DrizzleSqliteDODatabase } from "drizzle-orm/durable-sqlite";
import { migrate } from "drizzle-orm/durable-sqlite/migrator";
import { count } from "drizzle-orm";
import { WasmMatch, initSync } from "../wasm/awbrn_server";
import matchWasmModule from "../wasm/awbrn_server_bg.wasm";
import { normalizeCaughtError, ok, type MatchResult } from "./match_protocol";
import type { MatchCreateResponse } from "../schemas";
import migrations from "../../drizzle/match/migrations";
import { matchEventsTable } from "../db/match";

let wasmInitialized = false;

function ensureMatchWasmInitialized(): void {
  if (wasmInitialized) {
    return;
  }

  initSync({ module: matchWasmModule });
  wasmInitialized = true;
}

export class MatchDurableObject extends DurableObject<CloudflareBindings> {
  private readonly db: DrizzleSqliteDODatabase;

  constructor(ctx: DurableObjectState, env: CloudflareBindings) {
    super(ctx, env);
    this.db = drizzle(ctx.storage);
    ctx.blockConcurrencyWhile(async () => {
      await migrate(this.db, migrations);
    });
  }

  async initializeMatch(setup: unknown): Promise<MatchResult<MatchCreateResponse>> {
    try {
      const matchId = extractMatchId(setup);

      if (this.hasPersistedEvents()) {
        return ok({ matchId, joinSlug: null });
      }

      ensureMatchWasmInitialized();
      new WasmMatch(setup);
      this.appendEvent("setup", setup);

      return ok({ matchId, joinSlug: null });
    } catch (error) {
      return normalizeCaughtError(error);
    }
  }

  private hasPersistedEvents(): boolean {
    const result = this.db.select({ value: count() }).from(matchEventsTable).get();
    return (result?.value ?? 0) > 0;
  }

  private appendEvent(kind: string, payload: unknown): void {
    this.db
      .insert(matchEventsTable)
      .values({
        kind,
        payload,
        createdAt: new Date(),
      })
      .run();
  }
}

function extractMatchId(setup: unknown): string {
  if (
    typeof setup === "object" &&
    setup !== null &&
    "matchId" in setup &&
    typeof setup.matchId === "string" &&
    setup.matchId.length > 0
  ) {
    return setup.matchId;
  }

  return "unknown";
}

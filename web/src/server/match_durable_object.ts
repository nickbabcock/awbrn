import { DurableObject } from "cloudflare:workers";
import { drizzle, DrizzleSqliteDODatabase } from "drizzle-orm/durable-sqlite";
import { migrate } from "drizzle-orm/durable-sqlite/migrator";
import { count } from "drizzle-orm";
import { WasmMatch, initSync } from "../wasm/awbrn_server";
import matchWasmModule from "../wasm/awbrn_server_bg.wasm";
import {
  normalizeCaughtError,
  ok,
  err,
  type MatchCreateResponse,
  type MatchResult,
} from "./match_protocol";
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
      if (this.hasPersistedEvents()) {
        return err(
          "matchAlreadyInitialized",
          "match durable object has already been initialized",
          409,
          { matchId: this.ctx.id.toString() },
        );
      }

      ensureMatchWasmInitialized();
      new WasmMatch(setup);
      this.appendEvent("setup", setup);

      return ok({ matchId: this.ctx.id.toString() });
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

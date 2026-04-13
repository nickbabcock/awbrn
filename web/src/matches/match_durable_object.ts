import { DurableObject } from "cloudflare:workers";
import { drizzle, DrizzleSqliteDODatabase } from "drizzle-orm/durable-sqlite";
import { migrate } from "drizzle-orm/durable-sqlite/migrator";
import { count, eq } from "drizzle-orm";
import { WasmMatch, initSync } from "#/wasm/awbrn_server.js";
import matchWasmModule from "../wasm/awbrn_server_bg.wasm";
import {
  initialMatchConnectionMessages,
  normalizeCaughtError,
  ok,
  type MatchResult,
} from "./match_protocol";
import { matchSetupSchema } from "./schemas";
import type { MatchCreateResponse, MatchSetup } from "./schemas";
import migrations from "../../drizzle/match/migrations";
import { matchEventsTable } from "#/db/match.ts";
import { getRequestSession } from "#/auth/auth.server.ts";

// TODO: replace with a concrete type once game actions are defined
type GameActionPayload = Record<string, unknown>;

type MatchEvent =
  | { kind: "setup"; payload: MatchSetup }
  | { kind: "action"; payload: GameActionPayload };

function parseMatchEvent(row: { kind: string; payload: unknown }): MatchEvent | null {
  switch (row.kind) {
    case "setup": {
      const result = matchSetupSchema.safeParse(row.payload);
      return result.success ? { kind: "setup", payload: result.data } : null;
    }
    case "action":
      // TODO: parse against a concrete schema when game actions are defined
      return { kind: "action", payload: row.payload as GameActionPayload };
    default:
      return null;
  }
}

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
  private wasmMatch: WasmMatch | null = null;

  constructor(ctx: DurableObjectState, env: CloudflareBindings) {
    super(ctx, env);
    this.db = drizzle(ctx.storage);
    ctx.blockConcurrencyWhile(async () => {
      await migrate(this.db, migrations);
    });
  }

  async fetch(request: Request): Promise<Response> {
    if (request.headers.get("Upgrade") === "websocket") {
      const session = await getRequestSession(request);
      if (!session) {
        return new Response("Unauthorized", { status: 401 });
      }

      const setup = this.readSetupEvent();
      if (!setup) {
        return new Response("Match not initialized", { status: 503 });
      }

      return this.handleWebSocketUpgrade(session.user.id, setup);
    }
    return new Response("Not found", { status: 404 });
  }

  async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    const { userId, slotIndex } = ws.deserializeAttachment() as {
      userId: string;
      slotIndex: number | null;
    };
    const game = this.loadGame();
    if (game === null) {
      ws.send(JSON.stringify({ type: "error", message: "match not initialized" }));
      return;
    }
    try {
      const text = typeof message === "string" ? message : new TextDecoder().decode(message);
      const action = JSON.parse(text) as GameActionPayload;
      if (slotIndex === null) {
        // Spectators cannot submit actions
        ws.send(JSON.stringify({ type: "error", message: "spectators cannot submit actions" }));
        return;
      }
      // TODO: call game.processAction(slotIndex, action), persist event, broadcast result
      void action;
      void userId;
      this.broadcast({ type: "ack" });
    } catch {
      ws.send(JSON.stringify({ type: "error", message: "invalid message" }));
    }
  }

  async webSocketClose(ws: WebSocket, code: number, reason: string): Promise<void> {
    ws.close(code, reason);
  }

  async webSocketError(_ws: WebSocket, error: unknown): Promise<void> {
    console.error("WebSocket error in match DO:", error);
  }

  async initializeMatch(setup: MatchSetup): Promise<MatchResult<MatchCreateResponse>> {
    try {
      const matchId = extractMatchId(setup);

      if (this.hasPersistedEvents()) {
        return ok({ matchId, joinSlug: null });
      }

      ensureMatchWasmInitialized();
      this.wasmMatch = new WasmMatch(setup);
      this.appendEvent({ kind: "setup", payload: setup });

      return ok({ matchId, joinSlug: null });
    } catch (error) {
      return normalizeCaughtError(error);
    }
  }

  private handleWebSocketUpgrade(userId: string, setup: MatchSetup): Response {
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair) as [WebSocket, WebSocket];
    const slotIndex = setup.players.findIndex((p) => p.userId === userId);
    const playerSlotIndex = slotIndex >= 0 ? slotIndex : null;
    this.ctx.acceptWebSocket(server);
    server.serializeAttachment({ userId, slotIndex: playerSlotIndex });
    for (const message of initialMatchConnectionMessages(setup, playerSlotIndex)) {
      server.send(JSON.stringify(message));
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  private readSetupEvent(): MatchSetup | null {
    const row = this.db.select().from(matchEventsTable).where(eq(matchEventsTable.seq, 1)).get();
    if (!row) {
      return null;
    }
    const event = parseMatchEvent(row);
    return event?.kind === "setup" ? event.payload : null;
  }

  private loadGame(): WasmMatch | null {
    if (this.wasmMatch !== null) {
      return this.wasmMatch;
    }
    const setup = this.readSetupEvent();
    if (!setup) {
      return null;
    }
    ensureMatchWasmInitialized();
    try {
      this.wasmMatch = new WasmMatch(setup);
      return this.wasmMatch;
    } catch {
      return null;
    }
  }

  private broadcast(message: unknown): void {
    const text = JSON.stringify(message);
    for (const ws of this.ctx.getWebSockets()) {
      try {
        ws.send(text);
      } catch {
        // ignore closed connections
      }
    }
  }

  private hasPersistedEvents(): boolean {
    const result = this.db.select({ value: count() }).from(matchEventsTable).get();
    return (result?.value ?? 0) > 0;
  }

  private appendEvent(event: MatchEvent): void {
    this.db
      .insert(matchEventsTable)
      .values({
        kind: event.kind,
        payload: event.payload,
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

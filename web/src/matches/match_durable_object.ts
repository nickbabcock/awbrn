import { DurableObject } from "cloudflare:workers";
import { drizzle, DrizzleSqliteDODatabase } from "drizzle-orm/durable-sqlite";
import { migrate } from "drizzle-orm/durable-sqlite/migrator";
import { asc, count, eq } from "drizzle-orm";
import { WasmMatch, initSync } from "#/wasm/awbrn_server.js";
import matchWasmModule from "../wasm/awbrn_server_bg.wasm";
import {
  initialMatchConnectionMessages,
  normalizeCaughtError,
  ok,
  type MatchGameState,
  type MatchResult,
  type WasmActionResponse,
} from "./match_protocol";
import { matchSetupSchema } from "./schemas";
import type { MatchCreateResponse, MatchSetup } from "./schemas";
import migrations from "../../drizzle/match/migrations";
import { matchEventsTable } from "#/db/match.ts";
import { getRequestSession } from "#/auth/auth.server.ts";

interface WebSocketAttachment {
  userId: string;
  slotIndex: number | null;
}

type MatchEvent = { kind: "setup"; payload: MatchSetup } | { kind: "action"; payload: unknown };

function parseMatchEvent(row: { kind: string; payload: unknown }): MatchEvent | null {
  switch (row.kind) {
    case "setup": {
      const result = matchSetupSchema.safeParse(row.payload);
      return result.success ? { kind: "setup", payload: result.data } : null;
    }
    case "action":
      return { kind: "action", payload: row.payload };
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
    const { slotIndex } = deserializeAttachment(ws);
    const game = this.loadGame();
    if (game === null) {
      sendJson(ws, { type: "error", message: "match not initialized" });
      return;
    }

    let command: unknown;
    try {
      const text = typeof message === "string" ? message : new TextDecoder().decode(message);
      command = JSON.parse(text);
    } catch {
      sendJson(ws, { type: "error", message: "invalid message" });
      return;
    }

    if (slotIndex === null) {
      sendJson(ws, { type: "error", message: "spectators cannot submit actions" });
      return;
    }

    try {
      const response = game.process_action(slotIndex, command);
      try {
        this.appendEvent({ kind: "action", payload: response.storedActionEvent });
      } catch (error) {
        this.restoreGameFromPersistedEvents();
        throw error;
      }
      this.broadcastActionResponse(response);
    } catch (error) {
      const failure = normalizeCaughtError(error);
      sendJson(ws, { type: "error", message: failure.error.message });
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
    const game = this.loadGame();
    if (game === null) {
      return new Response("Match not initialized", { status: 503 });
    }

    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair) as [WebSocket, WebSocket];
    const slotIndex = setup.players.findIndex((p) => p.userId === userId);
    const playerSlotIndex = slotIndex >= 0 ? slotIndex : null;
    let gameState: MatchGameState | null = null;
    let spectatorNotice: Parameters<typeof initialMatchConnectionMessages>[3] = null;

    try {
      if (playerSlotIndex !== null) {
        gameState = game.playerGameState(playerSlotIndex);
      } else {
        gameState = game.spectatorGameState().gameState;
      }
    } catch (error) {
      const failure = normalizeCaughtError(error);
      return new Response(failure.error.message, { status: failure.error.httpStatus });
    }

    if (playerSlotIndex === null && gameState === null && setup.fogEnabled) {
      spectatorNotice = { type: "spectatorNotice", fogActive: true };
    }

    this.ctx.acceptWebSocket(server);
    server.serializeAttachment({ userId, slotIndex: playerSlotIndex });
    for (const message of initialMatchConnectionMessages(
      setup,
      playerSlotIndex,
      gameState,
      spectatorNotice,
    )) {
      sendJson(server, message);
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
      const actionEvents = this.readActionEvents();
      this.wasmMatch = WasmMatch.reconstructFromEvents(setup, actionEvents);
      return this.wasmMatch;
    } catch {
      return null;
    }
  }

  private restoreGameFromPersistedEvents(): void {
    this.wasmMatch = null;
    try {
      this.loadGame();
    } catch (error) {
      console.error("Failed to restore match state after append failure:", error);
    }
  }

  private broadcastActionResponse(response: WasmActionResponse): void {
    for (const target of this.ctx.getWebSockets()) {
      try {
        const { slotIndex } = deserializeAttachment(target);
        if (slotIndex === null) {
          if (response.spectatorMessage) {
            sendJson(target, response.spectatorMessage);
          }
          continue;
        }

        const message = response.playerMessagesBySlot.get(String(slotIndex));
        if (message) {
          sendJson(target, message);
        }
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

  private readActionEvents(): unknown[] {
    const rows = this.db.select().from(matchEventsTable).orderBy(asc(matchEventsTable.seq)).all();

    return rows
      .map(parseMatchEvent)
      .filter((event): event is { kind: "action"; payload: unknown } => event?.kind === "action")
      .map((event) => event.payload);
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

function deserializeAttachment(ws: WebSocket): WebSocketAttachment {
  const value = ws.deserializeAttachment() as Partial<WebSocketAttachment> | null;
  return {
    userId: typeof value?.userId === "string" ? value.userId : "unknown",
    slotIndex: typeof value?.slotIndex === "number" ? value.slotIndex : null,
  };
}

function sendJson(ws: WebSocket, message: unknown): void {
  ws.send(JSON.stringify(message));
}

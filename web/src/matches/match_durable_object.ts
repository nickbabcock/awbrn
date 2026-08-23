import { DurableObject } from "cloudflare:workers";
import { drizzle, DrizzleSqliteDODatabase } from "drizzle-orm/durable-sqlite";
import { drizzle as drizzleD1 } from "drizzle-orm/d1";
import { migrate } from "drizzle-orm/durable-sqlite/migrator";
import { and, asc, count, eq, isNull } from "drizzle-orm";
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
import { matchResults, matches } from "#/db/global.ts";
import { getRequestSession } from "#/auth/auth.server.ts";
import { ownedSlotIndices, selectOwnedPerspectiveSlot } from "./hotseat.ts";
import { matchResultRows } from "./match_completion.ts";
import { uploadMatchReplay } from "./replay_archive.ts";

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

/** Retry delay for an unwritten result. */
const RESULT_ALARM_DELAY_MS = 10_000;

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

      const response = this.handleWebSocketUpgrade(session.user.id, setup);
      // Retry result writes because the event log is durable.
      this.ctx.waitUntil(this.recordResultInBackground(setup));
      return response;
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
      const setup = this.readSetupEvent();
      if (!setup) {
        throw new Error("match setup disappeared after processing an action");
      }
      this.broadcastActionResponse(response, setup, game);
      await this.recordResultInBackground(setup);
    } catch (error) {
      const failure = normalizeCaughtError(error);
      sendJson(ws, { type: "error", message: failure.error.message });
    }
  }

  /** Retry a terminal result write after a database failure. */
  async alarm(): Promise<void> {
    const setup = this.readSetupEvent();
    if (!setup) {
      return;
    }
    await this.recordResultIfFinished(setup);
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
    const ownedSlots = ownedSlotIndices(setup, userId);
    let playerSlotIndex: number | null = null;
    let gameState: MatchGameState | null = null;
    let spectatorNotice: Parameters<typeof initialMatchConnectionMessages>[3] = null;

    try {
      playerSlotIndex = this.resolveConnectionSlot(game, ownedSlots);
      if (playerSlotIndex !== null) {
        gameState = game.playerGameState(playerSlotIndex);
      } else {
        gameState = game.spectatorGameState().gameState;
      }
    } catch (error) {
      const failure = normalizeCaughtError(error);
      console.error("Failed to prepare match WebSocket upgrade:", {
        matchId: setup.matchId,
        ownedSlots,
        playerSlotIndex,
        error: failure.error,
        cause: error,
      });
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

  private resolveConnectionSlot(game: WasmMatch, ownedSlots: number[]): number | null {
    const lowestOwnedSlot = ownedSlots[0] ?? null;
    if (lowestOwnedSlot === null) return null;
    const activePlayerSlot = game.playerGameState(lowestOwnedSlot).activePlayerSlot;
    return selectOwnedPerspectiveSlot(ownedSlots, activePlayerSlot, this.readActionEvents());
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

  private broadcastActionResponse(
    response: WasmActionResponse,
    setup: MatchSetup,
    game: WasmMatch,
  ): void {
    const activePlayerSlot = Object.values(response.playerMessagesBySlot)[0]?.activePlayerSlot;

    for (const target of this.ctx.getWebSockets()) {
      try {
        const { userId, slotIndex } = deserializeAttachment(target);
        if (slotIndex === null) {
          if (response.spectatorMessage) {
            sendJson(target, response.spectatorMessage);
          }
          continue;
        }

        const ownedSlots = ownedSlotIndices(setup, userId);
        if (
          activePlayerSlot !== undefined &&
          ownedSlots.includes(activePlayerSlot) &&
          activePlayerSlot !== slotIndex
        ) {
          target.serializeAttachment({ userId, slotIndex: activePlayerSlot });
          for (const message of initialMatchConnectionMessages(
            setup,
            activePlayerSlot,
            game.playerGameState(activePlayerSlot),
          )) {
            sendJson(target, message);
          }
          continue;
        }

        const message = response.playerMessagesBySlot[String(slotIndex)];
        if (message) {
          sendJson(target, message);
        }
      } catch {
        // Ignore closed connections.
      }
    }
  }

  /** Persist terminal results. Throws on failure so the alarm can retry. */
  private async recordResultIfFinished(setup: MatchSetup): Promise<void> {
    const game = this.loadGame();
    const results = game?.matchResults();
    if (!results) {
      return;
    }

    const rows = matchResultRows(setup, results);
    if (rows.length === 0) {
      return;
    }

    await this.ctx.storage.setAlarm(Date.now() + RESULT_ALARM_DELAY_MS);

    const db = drizzleD1(this.env.DB);
    const now = new Date();
    await uploadMatchReplay(this.env.CONTENT, setup, this.readActionEvents());
    await db.batch([
      db.insert(matchResults).values(rows).onConflictDoNothing(),
      db
        .update(matches)
        .set({ phase: "completed", completedAt: now, updatedAt: now })
        .where(and(eq(matches.id, setup.matchId), isNull(matches.completedAt))),
    ]);
    await this.ctx.storage.deleteAlarm();
  }

  /** Persist a result without reporting database errors to the player. */
  private async recordResultInBackground(setup: MatchSetup): Promise<void> {
    try {
      await this.recordResultIfFinished(setup);
    } catch (error) {
      console.error("Failed to record match results:", error);
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

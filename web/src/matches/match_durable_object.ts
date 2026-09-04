import { DurableObject } from "cloudflare:workers";
import { drizzle, DrizzleSqliteDODatabase } from "drizzle-orm/durable-sqlite";
import { drizzle as drizzleD1 } from "drizzle-orm/d1";
import { migrate } from "drizzle-orm/durable-sqlite/migrator";
import { and, asc, count, eq, isNull } from "drizzle-orm";
import { WasmMatch, WasmMatchReview } from "#/wasm/awbrn_server.js";
import type { GameCommand, MatchResults, StoredActionEvent } from "#/wasm/awbrn_server.js";
import {
  asReviewRequest,
  initialMatchConnectionMessages,
  matchClockMessage,
  normalizeCaughtError,
  ok,
  type MatchGameState,
  type MatchResult,
  type WasmActionResponse,
} from "./match_protocol";
import { matchSetupSchema } from "./schemas";
import type { MatchCreateResponse, MatchSetup } from "./schemas";
import type { ReviewRequest } from "./match_protocol";
import migrations from "../../drizzle/match/migrations";
import { matchEventsTable } from "#/db/match.ts";
import { matchResults, matches } from "#/db/global.ts";
import { getRequestSession } from "#/auth/auth.server.ts";
import { ownedSlotIndices, selectOwnedPerspectiveSlot } from "./hotseat.ts";
import { matchResultRows } from "./match_completion.ts";
import { uploadMatchReplay } from "./replay_archive.ts";
import { requireRateLimit } from "#/rate_limit.ts";
import { getMatchmakerStub } from "#/matchmaking/matchmaker_service.ts";
import { getRatingsStub } from "#/matchmaking/ratings_service.ts";
import {
  advanceClockProgress,
  commandEndsTurn,
  commandLeavesMatch,
  readClockProgress,
  startClockProgress,
} from "./match_clock.ts";
import type { ClockAction, ClockProgress, MatchClockState } from "./match_clock.ts";
import { turnFromClock, turnPublicationUpdate, type PublishedTurn } from "./turn_publication.ts";
import { getPlayerStubFrom } from "#/players/player_service.ts";

interface WebSocketAttachment {
  userId: string;
  slotIndex: number | null;
}

type MatchEvent =
  | { kind: "setup"; payload: MatchSetup }
  | { kind: "action"; payload: StoredActionEvent };

function parseMatchEvent(row: { kind: string; payload: unknown }): MatchEvent | null {
  switch (row.kind) {
    case "setup": {
      const result = matchSetupSchema.safeParse(row.payload);
      return result.success ? { kind: "setup", payload: result.data } : null;
    }
    case "action":
      // The engine wrote the row and reads it back, so it is taken at its
      // word. What the durable object reads out of it is guarded where it is
      // read, in `clockActionFromPayload`.
      return { kind: "action", payload: row.payload as StoredActionEvent };
    default:
      return null;
  }
}

/** The clock's reading of one recorded action, or null for a row it cannot read. */
function clockActionFromPayload(payload: unknown, at: number): ClockAction | null {
  const event = payload as Partial<StoredActionEvent> | null | undefined;
  const command = event?.command;
  if (typeof event?.player !== "number" || command === undefined) {
    return null;
  }
  return {
    slotIndex: event.player,
    endsTurn: commandEndsTurn(command),
    leavesMatch: commandLeavesMatch(command),
    at,
  };
}

/**
 * Turns the server plays for its own seats in one wake.
 *
 * A drain almost always ends at a person: a lobby keeps a seat for its host,
 * so the turn comes back within one lap of the table. The bound is for the
 * match that has lost its last person to an elimination and is now the server
 * playing itself, which is a match that has to finish without spending one
 * invocation's whole budget doing it. What is left over is picked up on the
 * next wake, which `nextWakeAt` asks for.
 */
const MAX_AI_TURNS_PER_WAKE = 16;

/**
 * How long a match waits before playing the turns it could not fit in one wake.
 *
 * A delay rather than an immediate wake, because the same wake is what a match
 * gets when a seat could not take its turn at all. Waking on that at once
 * would spend an invocation on every failure as fast as the platform allowed;
 * waking in a couple of seconds finishes an unattended endgame at a pace
 * nobody is waiting on and turns a stuck seat into a slow poll.
 */
const AI_TURN_ALARM_DELAY_MS = 2_000;

/** Retry delay for an unwritten result. */
const RESULT_ALARM_DELAY_MS = 10_000;

/** The first delay after a failed settle, which the platform's own retry starts at. */
const RETRY_BASE_MS = 2_000;

/** The longest a match waits to be looked at again after a failed settle. */
const MAX_RETRY_DELAY_MS = 5 * 60_000;

/**
 * How many times the platform retries an alarm handler that throws.
 *
 * It backs off from two seconds and then gives the alarm up for good, so the
 * last attempt is the last chance to leave one armed.
 */
const PLATFORM_ALARM_RETRIES = 6;

/** Marks the global database as holding this match's result. */
const RESULTS_RECORDED_KEY = "resultsRecorded";

/** The turn the global database was last told this match is on. */
const TURN_PUBLISHED_KEY = "turnPublished";

/** Wakes left for a ranked matchmaker that has not taken the wake yet. */
const MATCHMAKER_WAKE_KEY = "matchmakerWake";

/** How many wakes the pool's rating writer still owes. */
const RATINGS_WAKE_KEY = "ratingsWake";

/** How many times a refused ranked matchmaker wake is sent again. */
const MATCHMAKER_WAKE_ATTEMPTS = 6;

/**
 * Settles that failed in a row, which the retry delay is read from.
 *
 * It is kept in the durable object's own storage rather than in memory or in
 * the event log: the count has to survive the eviction or reset that a failure
 * may itself cause, and it is how the match is being run rather than anything
 * that happened in it.
 */
const SETTLE_FAILURES_KEY = "settleFailures";

/**
 * How long to wait before looking at a match that would not settle.
 *
 * The delay doubles with each failure. A deadline that has already passed is
 * what woke the alarm in the first place, so waking on it again would be a
 * loop that never backs off and never gets anywhere.
 */
function retryDelayMs(failures: number): number {
  return Math.min(RETRY_BASE_MS * 2 ** Math.max(0, failures - 1), MAX_RETRY_DELAY_MS);
}

/** Where a settle was reached from, which decides what a failure may do. */
interface SettleContext {
  /** True when the durable object woke on its own alarm. */
  fromAlarm: boolean;
  /** Retries the platform has already made of that alarm. */
  retryCount: number;
}

export class MatchDurableObject extends DurableObject<CloudflareBindings> {
  private readonly db: DrizzleSqliteDODatabase;
  private wasmMatch: WasmMatch | null = null;
  /**
   * A second reading of the match, standing wherever a viewer last asked to
   * read it.
   *
   * One cursor serves every viewer. The position it holds is the true board,
   * which each answer is projected from for the viewer who asked, so two
   * people reading different moments never see each other's view and never
   * cost more than the boundaries between them. It is built the first time
   * somebody asks to read the past and is not built at all for a match nobody
   * does.
   */
  private review: WasmMatchReview | null = null;
  /** Memo of the derived clock. `undefined` until a turn boundary is read. */
  private clock: MatchClockState | null | undefined = undefined;
  /**
   * The clock's running total over the action log.
   *
   * Built once from the log and carried forward action by action, so a match
   * that has been played for hours does not re-read its whole history on every
   * message.
   */
  private clockProgress: ClockProgress | undefined = undefined;
  /**
   * The publishes made so far, chained end to end.
   *
   * A durable object runs one turn of its own code at a time, so the chain is
   * all the ordering a publish needs: two of them cannot read the last
   * published turn, decide, and write over each other.
   */
  private publishing: Promise<void> = Promise.resolve();

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

      try {
        await Promise.all([
          requireRateLimit(this.env.WS_UPGRADE_RATE_LIMITER, `user:${session.user.id}`, 10),
          requireRateLimit(
            this.env.WS_UPGRADE_RATE_LIMITER,
            `match:${this.ctx.id.toString()}:user:${session.user.id}`,
            10,
          ),
        ]);
      } catch (response) {
        if (response instanceof Response) return response;
        throw response;
      }

      const setup = this.readSetupEvent();
      if (!setup) {
        return new Response("Match not initialized", { status: 503 });
      }

      const response = this.handleWebSocketUpgrade(session.user.id, setup);
      // Settle in the background because the event log is durable: an
      // unwritten result is retried, and a clock that ran out while the match
      // was asleep is enforced before anyone plays on.
      this.ctx.waitUntil(this.settle(setup, { fromAlarm: false, retryCount: 0 }));
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

    // A question about the match's past is not an order, so it is open to
    // somebody watching as well as to somebody playing.
    const reviewRequest = asReviewRequest(command);
    if (reviewRequest) {
      const reviewSetup = this.readSetupEvent();
      if (!reviewSetup) {
        sendJson(ws, { type: "error", message: "match not initialized" });
        return;
      }
      this.handleReviewRequest(ws, reviewRequest, reviewSetup, slotIndex);
      return;
    }

    if (slotIndex === null) {
      sendJson(ws, { type: "error", message: "spectators cannot submit actions" });
      return;
    }

    if (isTimeoutCommand(command)) {
      // The clock belongs to the host. A seat that wants out resigns instead.
      sendJson(ws, { type: "error", message: "only the server may time a seat out" });
      return;
    }

    const setup = this.readSetupEvent();
    if (!setup) {
      sendJson(ws, { type: "error", message: "match not initialized" });
      return;
    }

    try {
      // A seat cannot play on past its own deadline, however late the alarm
      // that should have removed it is.
      if (this.enforceClock(setup).includes(slotIndex)) {
        sendJson(ws, { type: "error", message: "your clock ran out" });
        await this.armAlarm(setup);
        return;
      }

      // The engine validates the command it is given; the cast marks the edge
      // where a websocket message stops being untrusted text.
      const response = game.process_action(slotIndex, command as GameCommand);
      try {
        this.appendEvent({ kind: "action", payload: response.storedActionEvent });
      } catch (error) {
        this.restoreGameFromPersistedEvents();
        throw error;
      }
      this.broadcastActionResponse(response, setup, game);
      this.broadcastClock(setup, game);
      this.playPendingAiTurns(setup, game);
      // The badge that counts a player's waiting matches reads the global
      // database, and nothing here waits on that read, so the publish stays
      // off the path the acting player is timed on. A publish that fails is
      // made again by the next settle, so the error is reported and not thrown
      // into the websocket message context.
      this.ctx.waitUntil(
        this.publishTurnState(setup).catch((error: unknown) => {
          console.error("Failed to publish the match turn:", error);
        }),
      );
      await this.recordResultInBackground(setup);
      await this.armAlarm(setup);
    } catch (error) {
      const failure = normalizeCaughtError(error);
      sendJson(ws, { type: "error", message: failure.error.message });
    }
  }

  /** Time out a seat whose clock ran out, and retry an unwritten result. */
  async alarm(alarmInfo?: AlarmInvocationInfo): Promise<void> {
    const setup = this.readSetupEvent();
    if (!setup) {
      return;
    }
    await this.settle(setup, {
      fromAlarm: true,
      retryCount: alarmInfo?.retryCount ?? 0,
    });
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

      this.wasmMatch = new WasmMatch(setup);
      this.appendEvent({ kind: "setup", payload: setup });
      // A match whose first seat is the server's opens on that seat's turn.
      // Playing it now is what stops its clock running down to a timeout
      // while everyone waits for a board that was never going to change.
      this.playPendingAiTurns(setup, this.wasmMatch);
      // The opening turn is published as the match starts, and a publish that
      // fails is made again by the next settle rather than failing the start.
      this.ctx.waitUntil(
        this.publishTurnState(setup).catch((error: unknown) => {
          console.error("Failed to publish the opening match turn:", error);
        }),
      );
      await this.armAlarm(setup);

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
    const clock = this.clockState(setup, game);
    if (clock) {
      sendJson(server, matchClockMessage(clock));
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  private resolveConnectionSlot(game: WasmMatch, ownedSlots: number[]): number | null {
    const lowestOwnedSlot = ownedSlots[0] ?? null;
    if (lowestOwnedSlot === null) return null;
    const activePlayerSlot = game.playerGameState(lowestOwnedSlot).activePlayerSlot;
    return selectOwnedPerspectiveSlot(ownedSlots, activePlayerSlot, this.readActionEvents());
  }

  /**
   * Answer one viewer's question about the match's past.
   *
   * The board that comes back is projected for the seat that asked, and for
   * nobody when a fogged match is asked by somebody holding no seat: such a
   * match has no public board, and answering with a seat's own would hand a
   * watcher what that seat can see.
   */
  private handleReviewRequest(
    ws: WebSocket,
    request: ReviewRequest,
    setup: MatchSetup,
    slotIndex: number | null,
  ): void {
    const review = this.loadReview(setup);
    if (review === null) {
      sendJson(ws, { type: "error", message: "this match cannot be reviewed" });
      return;
    }

    try {
      if (request.type === "reviewOutline") {
        sendJson(ws, { type: "reviewOutline", ...review.outline() });
        return;
      }
      const latest = review.latestIndex();
      const index = request.index === null ? latest : Math.min(request.index, latest);
      sendJson(ws, { type: "reviewState", ...review.seek(index, slotIndex) });
    } catch (error) {
      // A cursor that could not answer is a cursor that may have stopped
      // part-way through a rebuild. Drop it rather than answering the next
      // question from a position nothing put it in.
      this.review = null;
      const failure = normalizeCaughtError(error);
      console.error("Failed to answer a match review request:", {
        matchId: setup.matchId,
        request,
        error: failure.error,
      });
      sendJson(ws, { type: "error", message: failure.error.message });
    }
  }

  private loadReview(setup: MatchSetup): WasmMatchReview | null {
    if (this.review !== null) {
      return this.review;
    }
    try {
      this.review = new WasmMatchReview(setup, this.readActionEvents());
      return this.review;
    } catch (error) {
      console.error("Failed to open the match for review:", error);
      return null;
    }
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
    this.clock = undefined;
    // The cursor may have taken an action the log did not keep. It is built
    // again from the log by the next viewer who asks to read the match.
    this.review = null;
    // An append that failed may have left the running total ahead of the log.
    this.clockProgress = undefined;
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

  /**
   * Bring the match up to date and set the next alarm.
   *
   * The alarm has one use at a time: a running match wakes on its clock, and a
   * finished one wakes to retry a result the global database has not taken
   * yet. Both are settled here so neither can overwrite the other's alarm.
   */
  private async settle(setup: MatchSetup, context: SettleContext): Promise<void> {
    let failed = false;

    try {
      this.enforceClock(setup);
    } catch (error) {
      failed = true;
      console.error("Failed to enforce the match clock:", error);
    }

    try {
      const game = this.loadGame();
      if (game !== null) {
        this.playPendingAiTurns(setup, game);
      }
    } catch (error) {
      failed = true;
      console.error("Failed to play the server's seats:", error);
    }

    try {
      await this.recordResultIfFinished(setup);
    } catch (error) {
      failed = true;
      console.error("Failed to record match results:", error);
    }

    try {
      await this.publishTurnState(setup);
    } catch (error) {
      failed = true;
      console.error("Failed to publish the match turn:", error);
    }

    const failures = await this.recordSettleFailures(failed);

    try {
      await this.armAlarm(setup, failures);
    } catch (error) {
      // Scheduling is what failed, so this object has nothing left to retry
      // with: the platform's own alarm retry is holding the match's clock. It
      // gives up after `PLATFORM_ALARM_RETRIES`, and the next player to open
      // the match arms the alarm again on connect.
      console.error("Failed to set the next match alarm:", error);
      if (context.fromAlarm && context.retryCount + 1 < PLATFORM_ALARM_RETRIES) {
        throw error;
      }
    }
  }

  /** Count this settle, and report how many have failed in a row. */
  private async recordSettleFailures(failed: boolean): Promise<number> {
    const previous = (await this.ctx.storage.get<number>(SETTLE_FAILURES_KEY)) ?? 0;
    if (!failed) {
      if (previous !== 0) {
        await this.ctx.storage.delete(SETTLE_FAILURES_KEY);
      }
      return 0;
    }

    const failures = previous + 1;
    await this.ctx.storage.put(SETTLE_FAILURES_KEY, failures);
    return failures;
  }

  /**
   * Remove every seat whose clock ran out, and report which those were.
   *
   * More than one can be waiting when the match slept through several
   * deadlines, so this runs until a seat is left with time on it or the match
   * ends. The loop is bounded by the roster: a seat is removed each pass.
   */
  private enforceClock(setup: MatchSetup): number[] {
    const timedOut: number[] = [];
    const game = this.loadGame();
    if (game === null) {
      return timedOut;
    }

    for (let pass = 0; pass <= setup.players.length; pass += 1) {
      if (game.matchResults()) {
        return timedOut;
      }
      const clock = this.clockState(setup, game);
      if (clock === null || Date.now() < clock.deadlineAt) {
        return timedOut;
      }

      const response = game.process_action(clock.activeSlot, { type: "timeout" });
      try {
        this.appendEvent({ kind: "action", payload: response.storedActionEvent });
      } catch (error) {
        this.restoreGameFromPersistedEvents();
        throw error;
      }
      timedOut.push(clock.activeSlot);
      this.broadcastActionResponse(response, setup, game);
      this.broadcastClock(setup, game);
    }

    return timedOut;
  }

  /**
   * Play every turn the server owes its own seats, and send them on.
   *
   * Each command an opponent gets accepted is written down and broadcast the
   * way a person's is, because it is the same command through the same
   * authority. A person watching sees the turn play out rather than finding
   * the board already changed.
   *
   * Returns true when a turn is still owed, which is the bound being reached
   * rather than anything having gone wrong.
   */
  private playPendingAiTurns(setup: MatchSetup, game: WasmMatch): boolean {
    for (let turn = 0; turn < MAX_AI_TURNS_PER_WAKE; turn += 1) {
      const slot = game.pendingAiSlot();
      if (slot === undefined) {
        return false;
      }

      let actions;
      try {
        actions = game.runAiTurn(slot).actions;
      } catch (error) {
        // The seat could not take its turn. Leaving the match on that seat is
        // better than leaving it on a board nobody agreed to: the turn is
        // owed, the next wake asks for it again, and the seat's own clock is
        // what ends a match that never gets it.
        console.error("Failed to play a server-held seat:", {
          matchId: setup.matchId,
          slot,
          error,
        });
        return false;
      }

      for (const action of actions) {
        try {
          this.appendEvent({ kind: "action", payload: action.storedActionEvent });
        } catch (error) {
          this.restoreGameFromPersistedEvents();
          throw error;
        }
        this.broadcastActionResponse(action, setup, game);
      }
      this.broadcastClock(setup, game);
    }

    return game.pendingAiSlot() !== undefined;
  }

  /** Wake for the next deadline, for the next result retry, or to try again. */
  private async armAlarm(setup: MatchSetup, failures = 0): Promise<void> {
    let wakeAt: number | null;
    try {
      wakeAt = await this.nextWakeAt(setup, failures);
    } catch (error) {
      // A wake time that cannot be read is still a wake: the match is looked
      // at again rather than left asleep with a clock running on it.
      console.error("Failed to read the next match wake time:", error);
      wakeAt = Date.now() + retryDelayMs(failures + 1);
    }
    // Most actions leave the deadline where it was, so the alarm is read
    // before it is written and an unchanged one costs nothing.
    if ((await this.ctx.storage.getAlarm()) === wakeAt) {
      return;
    }
    if (wakeAt === null) {
      await this.ctx.storage.deleteAlarm();
      return;
    }
    await this.ctx.storage.setAlarm(wakeAt);
  }

  private async nextWakeAt(setup: MatchSetup, failures: number): Promise<number | null> {
    if (failures > 0) {
      return Date.now() + retryDelayMs(failures);
    }
    const game = this.loadGame();
    if (game === null) {
      return null;
    }
    if (game.matchResults()) {
      const [recorded, matchmakerWakes, ratingWakes] = await Promise.all([
        this.ctx.storage.get<boolean>(RESULTS_RECORDED_KEY),
        this.ctx.storage.get<number>(MATCHMAKER_WAKE_KEY),
        this.ctx.storage.get<number>(RATINGS_WAKE_KEY),
      ]);
      const settled = recorded === true && (matchmakerWakes ?? 0) <= 0 && (ratingWakes ?? 0) <= 0;
      return settled ? null : Date.now() + RESULT_ALARM_DELAY_MS;
    }
    // A turn the server still owes is a turn nobody is going to ask for, so
    // the match asks itself rather than waiting on a person who may not be
    // in it any more.
    if (game.pendingAiSlot() !== undefined) {
      return Date.now() + AI_TURN_ALARM_DELAY_MS;
    }
    return this.clockState(setup, game)?.deadlineAt ?? null;
  }

  /**
   * The clock as the recorded actions leave it, or null when it cannot be read.
   *
   * The event log is all a match durably holds, so the clock is read back from
   * it rather than kept beside it. The answer only changes at a turn boundary,
   * which is what the memo below tracks.
   */
  private clockState(setup: MatchSetup, game: WasmMatch): MatchClockState | null {
    if (this.clock !== undefined) {
      return this.clock;
    }
    this.clock = this.computeClockState(setup, game);
    return this.clock;
  }

  private computeClockState(setup: MatchSetup, game: WasmMatch): MatchClockState | null {
    const progress = this.clockProgressState(setup);
    if (progress === undefined) {
      return null;
    }
    let activeSlot: number;
    try {
      // Any seat reports the same active player; slot zero always exists.
      activeSlot = game.playerGameState(0).activePlayerSlot;
    } catch (error) {
      console.error("Failed to read the active seat for the match clock:", error);
      return null;
    }
    return readClockProgress(progress, activeSlot);
  }

  /** The running total, replayed from the log the first time it is asked for. */
  private clockProgressState(setup: MatchSetup): ClockProgress | undefined {
    if (this.clockProgress !== undefined) {
      return this.clockProgress;
    }
    const startedAt = this.readMatchStartedAt();
    if (startedAt === null) {
      // The match has no setup event yet, so nothing is memoized: the first
      // turn opens when that event is written.
      return undefined;
    }
    const progress = startClockProgress(setup.clock, startedAt, setup.players.length);
    for (const action of this.readClockActions()) {
      advanceClockProgress(progress, action);
    }
    this.clockProgress = progress;
    return progress;
  }

  /** Tell everyone watching how much time the seats have left. */
  private broadcastClock(setup: MatchSetup, game: WasmMatch): void {
    const clock = this.clockState(setup, game);
    if (clock === null) {
      return;
    }
    for (const target of this.ctx.getWebSockets()) {
      try {
        sendJson(target, matchClockMessage(clock));
      } catch {
        // Ignore closed connections.
      }
    }
  }

  /** When the setup event was written, which is when the first turn opened. */
  private readMatchStartedAt(): number | null {
    const row = this.db.select().from(matchEventsTable).where(eq(matchEventsTable.seq, 1)).get();
    return row ? row.createdAt.getTime() : null;
  }

  private readClockActions(): ClockAction[] {
    const rows = this.db
      .select({ payload: matchEventsTable.payload, createdAt: matchEventsTable.createdAt })
      .from(matchEventsTable)
      .where(eq(matchEventsTable.kind, "action"))
      .orderBy(asc(matchEventsTable.seq))
      .all();

    return rows.flatMap((row) => {
      const action = clockActionFromPayload(row.payload, row.createdAt.getTime());
      return action === null ? [] : [action];
    });
  }

  /**
   * Tell the global database which seat this match is waiting on.
   *
   * The write is made only at a turn boundary: the turn is compared against
   * the last one published, and an action that leaves the same seat on the
   * move costs nothing. The published turn is recorded after the update lands,
   * so a publish that failed is made again by the next settle.
   */
  private async publishTurnState(setup: MatchSetup): Promise<void> {
    const publish = this.publishing.then(async () => {
      const game = this.loadGame();
      if (game === null) {
        return;
      }
      const turn = turnFromClock(this.clockState(setup, game), Boolean(game.matchResults()));
      const published = await this.ctx.storage.get<PublishedTurn>(TURN_PUBLISHED_KEY);
      const update = turnPublicationUpdate(turn, published);
      if (update === null) {
        return;
      }

      const db = drizzleD1(this.env.DB);
      const [row] = await db
        .update(matches)
        .set({
          activeSlotIndex: update.activeSlotIndex,
          turnDeadlineAt: update.turnDeadlineAt === null ? null : new Date(update.turnDeadlineAt),
        })
        .where(eq(matches.id, setup.matchId))
        // The name travels with the write because it is what a notification
        // has to say, and reading it here costs nothing over a second query.
        .returning({ name: matches.name });
      await this.ctx.storage.put(TURN_PUBLISHED_KEY, update);
      // Only now, with the count this announcement sends players to re-read
      // already written, so nobody can be told to look at a stale row.
      await this.announceTurnChange(setup, published ?? null, update, row?.name ?? "your match");
    });

    // A failed publish must not leave every publish after it refusing to run,
    // so the chain carries the ordering and the caller carries the error.
    this.publishing = publish.catch(() => {});
    await publish;
  }

  /**
   * Tell the players whose turn just started, or just ended, that it did.
   *
   * A player is reached through their own durable object rather than through
   * this one, because the player waiting on a match is by definition not
   * connected to it. The announcement is best effort: it is what saves a
   * player a wait, and the count it sends them to read is already written, so
   * one that does not arrive costs a refresh and never correctness.
   */
  private async announceTurnChange(
    setup: MatchSetup,
    previous: PublishedTurn | null,
    update: PublishedTurn,
    matchName: string,
  ): Promise<void> {
    const startedSlot = update.activeSlotIndex;
    const endedSlot = previous?.activeSlotIndex ?? null;
    const startedUserId = slotUserId(setup, startedSlot);
    const endedUserId = slotUserId(setup, endedSlot);

    const notifications: Promise<void>[] = [];
    if (startedUserId !== null) {
      notifications.push(
        getPlayerStubFrom(this.env.PLAYERS, startedUserId).notify({
          type: "turnStarted",
          matchId: setup.matchId,
          matchName,
          deadlineAt: update.turnDeadlineAt,
        }),
      );
    }
    // A hotseat match hands one player the seat they just left, and they are
    // told the turn started rather than told both things about themselves.
    if (endedUserId !== null && endedUserId !== startedUserId) {
      notifications.push(
        getPlayerStubFrom(this.env.PLAYERS, endedUserId).notify({
          type: "turnEnded",
          matchId: setup.matchId,
        }),
      );
    }

    const settled = await Promise.allSettled(notifications);
    for (const result of settled) {
      if (result.status === "rejected") {
        console.error("Failed to announce a turn change to a player:", result.reason);
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
    if ((await this.ctx.storage.get<boolean>(RESULTS_RECORDED_KEY)) === true) {
      await this.resumeRankedMatchmaking(setup, false);
      await this.wakeRatingWriter(setup, false);
      return;
    }

    const rows = matchResultRows(setup, results);
    if (rows.length > 0) {
      const db = drizzleD1(this.env.DB);
      const now = new Date();
      await uploadMatchReplay(this.env.CONTENT, setup, this.readActionEvents());
      await db.batch([
        db.insert(matchResults).values(rows).onConflictDoNothing(),
        db
          .update(matches)
          .set({
            phase: "completed",
            completedAt: now,
            updatedAt: now,
            // The match stops counting as one waiting on a player in the same
            // write that ends it. `publishTurnState` records that it has.
            activeSlotIndex: null,
            turnDeadlineAt: null,
          })
          .where(and(eq(matches.id, setup.matchId), isNull(matches.completedAt))),
      ]);
    }

    // A match with nothing to write is recorded too, so it stops being retried.
    await this.ctx.storage.put(RESULTS_RECORDED_KEY, true);

    // Announced once the match is durably over, and not before: the page
    // answers this by re-reading a record that has to already say the match
    // ended and by fetching an archive that has to already be stored. A write
    // that failed throws above this and is retried, so a retry announces the
    // end once rather than again. A socket that is not open to hear it loses
    // nothing, because the record is what the page reads when it comes back.
    this.broadcastMatchOver(results);
    await this.resumeRankedMatchmaking(setup, true);
    await this.wakeRatingWriter(setup, true);
  }

  /**
   * Wake the pool's rating writer so the result this match wrote is rated.
   *
   * The wake is counted and retried the way the matchmaker's is, for the same
   * reason: a refused wake must be sent again rather than lost. Nothing is
   * carried in it. The rating writer reads `match_results` for the work, so a
   * wake which never lands only delays the rating until the next match of the
   * pool ends.
   */
  private async wakeRatingWriter(setup: MatchSetup, first: boolean): Promise<void> {
    const { pool } = setup;
    if (pool == null) {
      return;
    }
    const wakesLeft = first
      ? MATCHMAKER_WAKE_ATTEMPTS
      : ((await this.ctx.storage.get<number>(RATINGS_WAKE_KEY)) ?? 0);
    if (wakesLeft <= 0) {
      return;
    }

    await this.ctx.storage.put(RATINGS_WAKE_KEY, wakesLeft - 1);
    this.ctx.waitUntil(
      getRatingsStub(this.env.RATINGS, pool)
        .kick(pool)
        .then(() => this.ctx.storage.delete(RATINGS_WAKE_KEY))
        .catch((error: unknown) => {
          console.error("Failed to wake the ranked rating writer:", error);
        }),
    );
  }

  /**
   * Wake the pool's matchmaker so the seats this match freed are paired again.
   *
   * The count of wakes left is written to storage before a wake is sent, and
   * it is deleted only when the matchmaker takes one. The match keeps its
   * alarm while wakes are left, so a refused wake is sent again instead of
   * being lost with the recorded result. The kick is safe to repeat.
   */
  private async resumeRankedMatchmaking(setup: MatchSetup, first: boolean): Promise<void> {
    const { pool, season } = setup;
    if (pool == null || season == null) {
      return;
    }
    const wakesLeft = first
      ? MATCHMAKER_WAKE_ATTEMPTS
      : ((await this.ctx.storage.get<number>(MATCHMAKER_WAKE_KEY)) ?? 0);
    if (wakesLeft <= 0) {
      return;
    }

    await this.ctx.storage.put(MATCHMAKER_WAKE_KEY, wakesLeft - 1);
    this.ctx.waitUntil(
      getMatchmakerStub(this.env.MATCHMAKERS, season, pool)
        .kick(pool, season)
        .then(() => this.ctx.storage.delete(MATCHMAKER_WAKE_KEY))
        .catch((error: unknown) => {
          console.error("Failed to resume ranked matchmaking:", error);
        }),
    );
  }

  /** Persist a result without reporting database errors to the player. */
  /** Tell every open socket the match is over and how it ended. */
  private broadcastMatchOver(results: MatchResults): void {
    for (const target of this.ctx.getWebSockets()) {
      try {
        sendJson(target, { type: "matchOver", results });
      } catch {
        // Ignore closed connections.
      }
    }
  }

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
    const createdAt = new Date();
    this.db
      .insert(matchEventsTable)
      .values({
        kind: event.kind,
        payload: event.payload,
        createdAt,
      })
      .run();

    // A viewer reading an earlier turn stays where they are reading; only the
    // end of their cursor's log moves. A cursor that cannot take the action is
    // dropped and built again by the next viewer, because a cursor that has
    // missed an action would answer for a match that never happened.
    if (event.kind === "action" && this.review !== null) {
      try {
        this.review.append(event.payload);
      } catch (error) {
        console.error("Failed to record an action for review:", error);
        this.review = null;
      }
    }

    // A seat can lose its turn to an action that does not say so, by routing
    // itself, so any recorded event can move the clock and the memo is dropped
    // for all of them. The running total behind it takes the new action rather
    // than being dropped, which is what keeps the log off the hot path.
    this.clock = undefined;
    if (event.kind === "action" && this.clockProgress) {
      const action = clockActionFromPayload(event.payload, createdAt.getTime());
      if (action === null) {
        this.clockProgress = undefined;
      } else {
        advanceClockProgress(this.clockProgress, action);
      }
    }
  }

  private readActionEvents(): StoredActionEvent[] {
    const rows = this.db.select().from(matchEventsTable).orderBy(asc(matchEventsTable.seq)).all();

    return rows
      .map(parseMatchEvent)
      .filter(
        (event): event is { kind: "action"; payload: StoredActionEvent } =>
          event?.kind === "action",
      )
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

/** Who holds a seat, or null for no seat and for one nobody holds. */
function slotUserId(setup: MatchSetup, slotIndex: number | null): string | null {
  if (slotIndex === null) {
    return null;
  }
  return setup.players[slotIndex]?.userId ?? null;
}

function deserializeAttachment(ws: WebSocket): WebSocketAttachment {
  const value = ws.deserializeAttachment() as Partial<WebSocketAttachment> | null;
  return {
    userId: typeof value?.userId === "string" ? value.userId : "unknown",
    slotIndex: typeof value?.slotIndex === "number" ? value.slotIndex : null,
  };
}

function isTimeoutCommand(command: unknown): boolean {
  return (
    typeof command === "object" &&
    command !== null &&
    "type" in command &&
    command.type === "timeout"
  );
}

function sendJson(ws: WebSocket, message: unknown): void {
  ws.send(JSON.stringify(message));
}

import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type {
  CombatEventMessage,
  GameCommand as WasmGameCommand,
  MatchReviewBoundary,
  MatchReviewOutline,
  MatchReviewState,
  MatchGameState as WasmMatchGameState,
  MatchResults,
  ObservedTransition,
  PlayerUpdateMessage as WasmPlayerUpdateMessage,
  PublicPlayerState as WasmPublicPlayerState,
  SpectatorMessage,
  TurnChangeMessage,
  UnitMovedMessage,
  WasmActionResponse as GeneratedWasmActionResponse,
  WireVisibleTerrain,
  WireVisibleUnit,
} from "#/wasm/awbrn_server.js";
import type { MatchClockState } from "./match_clock.ts";
import type { MatchSetup } from "./schemas.ts";

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

export type PublicPlayerState = WasmPublicPlayerState;
export type VisibleUnit = WireVisibleUnit;
export type VisibleTerrain = WireVisibleTerrain;
export type MatchGameState = WasmMatchGameState;
export type LiveTransition = ObservedTransition;
/** What the engine decided for every seat, once a match is over. */
export type { MatchResults, SeatResult } from "#/wasm/awbrn_server.js";

export interface InitialBoardMessage {
  type: "initialBoard";
  mapId: string;
  revision: number;
  map: AwbrnMapDocument;
  gameState: MatchGameState | null;
}

export interface ConnectedMessage {
  type: "connected";
  slotIndex: number | null;
}

export interface AckMessage {
  type: "ack";
}

/**
 * A viewer asking for every boundary the match can be read at.
 *
 * The answer carries no board, so it is asked for once and holds for the rest
 * of the match: what arrives after it are the actions the viewer watches
 * happen.
 */
export interface ReviewOutlineRequest {
  type: "reviewOutline";
}

/** A viewer asking to be shown one boundary of the match. */
export interface ReviewSeekRequest {
  type: "reviewSeek";
  /**
   * How many actions had been taken, so `0` opens the match, and `null` is
   * the match as it stands.
   *
   * The end of a match that is still being played is a moving target, so a
   * viewer coming back to it names it rather than counting to it: a number
   * that was the end when the viewer read it may be an action behind by the
   * time the question arrives.
   */
  index: number | null;
}

/**
 * What a viewer may ask about a match's past.
 *
 * Neither of these is an order, so both are open to somebody watching as well
 * as to somebody playing. Neither names an action either: a viewer asks for a
 * moment and is answered with the board they are entitled to see at it, which
 * is what keeps a fogged match's history hidden while it is still being
 * played.
 */
export type ReviewRequest = ReviewOutlineRequest | ReviewSeekRequest;

export type ReviewBoundary = MatchReviewBoundary;

export interface ReviewOutlineMessage extends MatchReviewOutline {
  type: "reviewOutline";
}

export interface ReviewStateMessage extends MatchReviewState {
  type: "reviewState";
}

/** Whether a websocket message from a viewer is a question about the past. */
export function asReviewRequest(message: unknown): ReviewRequest | null {
  if (typeof message !== "object" || message === null || !("type" in message)) {
    return null;
  }
  if (message.type === "reviewOutline") {
    return { type: "reviewOutline" };
  }
  if (message.type === "reviewSeek" && "index" in message) {
    if (message.index === null) {
      return { type: "reviewSeek", index: null };
    }
    if (
      typeof message.index === "number" &&
      Number.isInteger(message.index) &&
      message.index >= 0
    ) {
      return { type: "reviewSeek", index: message.index };
    }
  }
  return null;
}

export interface ErrorMessage {
  type: "error";
  message: string;
}

/**
 * How much time the seats have left, in milliseconds.
 *
 * Sent when a client connects and after every action, so a client counts down
 * from the server's numbers instead of keeping a clock of its own.
 */
export interface MatchClockMessage {
  type: "clock";
  activeSlot: number;
  /** When the active seat runs out, as a unix timestamp in milliseconds. */
  deadlineAt: number;
  /** Time left for each seat, by slot index. */
  banksMs: Record<number, number>;
}

/** Every command the engine accepts, whoever submits it. */
export type MatchCommand = WasmGameCommand;

/**
 * A command a seat may send over the live-match websocket.
 *
 * The clock belongs to the host, so `timeout` is not among them: a seat that
 * wants out resigns. The match durable object rejects one on the player
 * websocket as well, because a websocket carries whatever the far end writes.
 *
 * Every command but `resign` is an order the seat holding the turn gives. A
 * seat may resign on anybody's turn, which is why it is the one command the
 * page offers a player who is not on the move.
 */
export type PlayerCommand = Exclude<MatchCommand, { type: "timeout" }>;
export type ActivatePowerCommand = Extract<PlayerCommand, { type: "activatePower" }>;
export type EndTurnCommand = Extract<PlayerCommand, { type: "endTurn" }>;
export type ResignCommand = Extract<PlayerCommand, { type: "resign" }>;

export type UnitMoved = UnitMovedMessage;
export type TurnChange = TurnChangeMessage;
export type CombatEvent = CombatEventMessage;
export type PlayerUpdateMessage = WasmPlayerUpdateMessage;
export type SpectatorNoticeMessage = Extract<SpectatorMessage, { type: "spectatorNotice" }>;
export type SpectatorStateMessage = Extract<SpectatorMessage, { type: "spectatorState" }>;

/**
 * The match is over, with what the engine decided for every seat.
 *
 * This is announced once, at the moment the result is recorded, so a player
 * watching the board learns the match ended from the match itself rather than
 * from the absence of anything happening next. It carries no rating: a rating
 * is applied after the match, by the pool that owns it, and reaches the player
 * on their own socket.
 *
 * The results are public. Everything a fogged match was hiding is decided by
 * the time this is sent, so a spectator is told the same as a player.
 */
export interface MatchOverMessage {
  type: "matchOver";
  results: MatchResults;
}

export type MatchWebSocketMessage =
  | InitialBoardMessage
  | ConnectedMessage
  | AckMessage
  | ErrorMessage
  | PlayerUpdateMessage
  | SpectatorNoticeMessage
  | SpectatorStateMessage
  | ReviewOutlineMessage
  | ReviewStateMessage
  | MatchOverMessage
  | MatchClockMessage;

export type WasmActionResponse = GeneratedWasmActionResponse;

export function matchClockMessage(clock: MatchClockState): MatchClockMessage {
  return {
    type: "clock",
    activeSlot: clock.activeSlot,
    deadlineAt: clock.deadlineAt,
    banksMs: clock.banksMs,
  };
}

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

export function initialMatchConnectionMessages(
  setup: Pick<MatchSetup, "mapId" | "revision" | "map">,
  slotIndex: number | null,
  gameState: MatchGameState | null,
  spectatorNotice: SpectatorNoticeMessage | null = null,
): MatchWebSocketMessage[] {
  const messages: MatchWebSocketMessage[] = [
    {
      type: "initialBoard",
      mapId: setup.mapId,
      revision: setup.revision,
      map: setup.map,
      gameState,
    },
  ];

  if (spectatorNotice) {
    messages.push(spectatorNotice);
  }

  messages.push({
    type: "connected",
    slotIndex,
  });

  return messages;
}

const WASM_ERROR_PREFIX = "AWBRN_MATCH_ERROR:";

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

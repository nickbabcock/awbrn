import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type {
  CombatEventMessage,
  GameCommand as WasmGameCommand,
  MatchGameState as WasmMatchGameState,
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
 */
export type PlayerCommand = Exclude<MatchCommand, { type: "timeout" }>;
export type ActivatePowerCommand = Extract<PlayerCommand, { type: "activatePower" }>;
export type EndTurnCommand = Extract<PlayerCommand, { type: "endTurn" }>;

export type UnitMoved = UnitMovedMessage;
export type TurnChange = TurnChangeMessage;
export type CombatEvent = CombatEventMessage;
export type PlayerUpdateMessage = WasmPlayerUpdateMessage;
export type SpectatorNoticeMessage = Extract<SpectatorMessage, { type: "spectatorNotice" }>;
export type SpectatorStateMessage = Extract<SpectatorMessage, { type: "spectatorState" }>;

export type MatchWebSocketMessage =
  | InitialBoardMessage
  | ConnectedMessage
  | AckMessage
  | ErrorMessage
  | PlayerUpdateMessage
  | SpectatorNoticeMessage
  | SpectatorStateMessage
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

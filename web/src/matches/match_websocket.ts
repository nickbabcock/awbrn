import { useCallback, useEffect, useRef, useState } from "react";
import type { MatchWebSocketMessage, PlayerCommand, ReviewRequest } from "./match_protocol.ts";

export type MatchWebSocketStatus = "connecting" | "connected" | "disconnected" | "error";

export interface MatchWebSocket {
  status: MatchWebSocketStatus;
  /** Returns whether the command was written to an open socket. */
  /**
   * Send one order, or one question about the match's past.
   *
   * Both go up the same socket because both are answered against the same
   * match, and the socket is what says who is asking.
   */
  sendMessage: (message: PlayerCommand | ReviewRequest) => boolean;
  /**
   * Retry immediately instead of waiting out the backoff, which reaches 30
   * seconds. A player who can see the connection is down should not have to
   * wait for a timer they cannot see.
   */
  reconnect: () => void;
}

const BASE_RECONNECT_DELAY_MS = 1_000;
const MAX_RECONNECT_DELAY_MS = 30_000;

export function useMatchWebSocket(
  matchId: string,
  onMessage: (msg: MatchWebSocketMessage) => void,
): MatchWebSocket {
  const [status, setStatus] = useState<MatchWebSocketStatus>("connecting");
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttemptRef = useRef(0);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const unmountedRef = useRef(false);
  const onMessageRef = useRef(onMessage);
  onMessageRef.current = onMessage;

  const connect = useCallback(() => {
    if (unmountedRef.current) return;
    if (wsRef.current?.readyState === WebSocket.CONNECTING) return;

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${protocol}//${window.location.host}/api/matches/${matchId}/ws`;
    const ws = new WebSocket(url);
    wsRef.current = ws;
    setStatus("connecting");

    ws.addEventListener("open", () => {
      if (unmountedRef.current || wsRef.current !== ws) return;
      reconnectAttemptRef.current = 0;
      setStatus("connected");
    });

    ws.addEventListener("message", (event: MessageEvent<string>) => {
      if (unmountedRef.current || wsRef.current !== ws) return;
      try {
        const parsed = JSON.parse(event.data) as MatchWebSocketMessage;
        onMessageRef.current(parsed);
      } catch {
        // ignore unparseable frames
      }
    });

    ws.addEventListener("close", () => {
      if (unmountedRef.current || wsRef.current !== ws) return;
      wsRef.current = null;
      setStatus("disconnected");
      const attempt = reconnectAttemptRef.current;
      const delay = Math.min(BASE_RECONNECT_DELAY_MS * 2 ** attempt, MAX_RECONNECT_DELAY_MS);
      const jitter = delay * 0.2 * Math.random();
      reconnectAttemptRef.current = attempt + 1;
      reconnectTimerRef.current = setTimeout(connect, delay + jitter);
    });

    ws.addEventListener("error", () => {
      if (unmountedRef.current || wsRef.current !== ws) return;
      setStatus("error");
      // close fires after error, which triggers the reconnect
    });
  }, [matchId]);

  useEffect(() => {
    unmountedRef.current = false;
    reconnectAttemptRef.current = 0;
    connect();

    return () => {
      unmountedRef.current = true;
      if (reconnectTimerRef.current !== null) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
      wsRef.current?.close();
      wsRef.current = null;
    };
  }, [connect]);

  const sendMessage = useCallback((message: PlayerCommand | ReviewRequest) => {
    const ws = wsRef.current;
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(message));
      return true;
    }
    return false;
  }, []);

  const reconnect = useCallback(() => {
    if (reconnectTimerRef.current !== null) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
    // A hand-made attempt restarts the backoff, so a later drop retries
    // quickly again rather than inheriting the previous delay.
    reconnectAttemptRef.current = 0;
    const ws = wsRef.current;
    wsRef.current = null;
    ws?.close();
    connect();
  }, [connect]);

  return { status, sendMessage, reconnect };
}

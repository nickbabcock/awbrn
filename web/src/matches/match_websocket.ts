import { useCallback, useEffect, useRef, useState } from "react";

export type MatchWebSocketStatus = "connecting" | "connected" | "disconnected" | "error";

export interface MatchWebSocketMessage {
  type: string;
  [key: string]: unknown;
}

export interface MatchWebSocket {
  status: MatchWebSocketStatus;
  sendMessage: (message: unknown) => void;
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

  const sendMessage = useCallback((message: unknown) => {
    const ws = wsRef.current;
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(message));
    }
  }, []);

  return { status, sendMessage };
}

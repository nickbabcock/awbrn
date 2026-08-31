import { useCallback, useEffect, useRef, useState } from "react";
import type { PlayerSocketMessage } from "./player_protocol.ts";

export type PlayerSocketStatus = "connecting" | "connected" | "disconnected";

const BASE_RECONNECT_DELAY_MS = 1_000;
const MAX_RECONNECT_DELAY_MS = 30_000;

/**
 * How often a held socket is pinged.
 *
 * The answer is written by the runtime without waking the player's durable
 * object, so this costs nothing to keep up and is only here to stop the
 * proxies between a tab and the edge from closing a socket they think is idle.
 */
const KEEPALIVE_INTERVAL_MS = 45_000;

/**
 * The socket a signed-in player holds for as long as a tab is open.
 *
 * There is one for the whole site rather than one for each match, so a player
 * hears that a match has moved without having it open. It reconnects on its
 * own, and a tab that is not being read says so, which is what decides whether
 * the player is sent a notification as well as told on the socket.
 */
export function usePlayerSocket(
  enabled: boolean,
  onMessage: (message: PlayerSocketMessage) => void,
): PlayerSocketStatus {
  const [status, setStatus] = useState<PlayerSocketStatus>(enabled ? "connecting" : "disconnected");
  const socketRef = useRef<WebSocket | null>(null);
  const attemptRef = useRef(0);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const keepaliveTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const stoppedRef = useRef(false);
  const onMessageRef = useRef(onMessage);
  onMessageRef.current = onMessage;

  const reportVisibility = useCallback(() => {
    const socket = socketRef.current;
    if (socket?.readyState !== WebSocket.OPEN) return;
    socket.send(
      JSON.stringify({ type: "visibility", visible: document.visibilityState === "visible" }),
    );
  }, []);

  const connect = useCallback(() => {
    if (stoppedRef.current) return;
    if (socketRef.current?.readyState === WebSocket.CONNECTING) return;

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(`${protocol}//${window.location.host}/api/player/ws`);
    socketRef.current = socket;
    setStatus("connecting");

    socket.addEventListener("open", () => {
      if (stoppedRef.current || socketRef.current !== socket) return;
      attemptRef.current = 0;
      setStatus("connected");
      // The tab may have been hidden while the socket was down, so what the
      // durable object knows is corrected as soon as it can hear it.
      reportVisibility();
    });

    socket.addEventListener("message", (event: MessageEvent<string>) => {
      if (stoppedRef.current || socketRef.current !== socket) return;
      if (event.data === "pong") return;
      try {
        onMessageRef.current(JSON.parse(event.data) as PlayerSocketMessage);
      } catch {
        // ignore unparseable frames
      }
    });

    socket.addEventListener("close", () => {
      if (stoppedRef.current || socketRef.current !== socket) return;
      socketRef.current = null;
      setStatus("disconnected");
      const attempt = attemptRef.current;
      const delay = Math.min(BASE_RECONNECT_DELAY_MS * 2 ** attempt, MAX_RECONNECT_DELAY_MS);
      attemptRef.current = attempt + 1;
      reconnectTimerRef.current = setTimeout(connect, delay + delay * 0.2 * Math.random());
    });

    socket.addEventListener("error", () => {
      // close follows error, and that is what schedules the retry.
    });
  }, [reportVisibility]);

  useEffect(() => {
    if (!enabled) {
      setStatus("disconnected");
      return;
    }

    stoppedRef.current = false;
    attemptRef.current = 0;
    connect();

    const onVisibilityChange = () => {
      reportVisibility();
      // Coming back to a tab whose socket dropped while it was hidden should
      // not wait out the backoff the player cannot see.
      if (document.visibilityState === "visible" && socketRef.current === null) {
        if (reconnectTimerRef.current !== null) {
          clearTimeout(reconnectTimerRef.current);
          reconnectTimerRef.current = null;
        }
        attemptRef.current = 0;
        connect();
      }
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    keepaliveTimerRef.current = setInterval(() => {
      const socket = socketRef.current;
      if (socket?.readyState === WebSocket.OPEN) {
        socket.send("ping");
      }
    }, KEEPALIVE_INTERVAL_MS);

    return () => {
      stoppedRef.current = true;
      document.removeEventListener("visibilitychange", onVisibilityChange);
      if (reconnectTimerRef.current !== null) clearTimeout(reconnectTimerRef.current);
      if (keepaliveTimerRef.current !== null) clearInterval(keepaliveTimerRef.current);
      reconnectTimerRef.current = null;
      keepaliveTimerRef.current = null;
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [connect, enabled, reportVisibility]);

  return status;
}

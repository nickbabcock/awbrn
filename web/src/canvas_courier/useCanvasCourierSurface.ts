import { useCallback, useEffect, useRef, useState } from "react";
import type { CanvasCourierController, CanvasCourierStatus } from "./types";

export function useCanvasCourierSurface({
  controller,
  onError,
}: {
  controller: CanvasCourierController | null;
  onError?: (error: Error) => void;
}) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const offscreenRef = useRef<OffscreenCanvas | null>(null);
  const onErrorRef = useRef(onError);
  const [status, setStatus] = useState<CanvasCourierStatus>({
    attached: false,
    error: null,
  });

  useEffect(() => {
    onErrorRef.current = onError;
  }, [onError]);

  const focus = useCallback(() => {
    canvasRef.current?.focus({ preventScroll: true });
  }, []);

  const blur = useCallback(() => {
    canvasRef.current?.blur();
  }, []);

  useEffect(() => {
    if (!controller) {
      setStatus((current) =>
        current.attached || current.error !== null ? { attached: false, error: null } : current,
      );
      return;
    }

    const canvas = canvasRef.current;
    const container = surfaceRef.current;
    if (!canvas || !container) {
      return;
    }

    offscreenRef.current ??= canvas.transferControlToOffscreen();

    let cancelled = false;

    controller
      .attachSurface({ canvas, container, offscreen: offscreenRef.current })
      .then(() => {
        if (cancelled) {
          return;
        }
        setStatus((current) =>
          current.attached && current.error === null ? current : { attached: true, error: null },
        );
      })
      .catch((error) => {
        if (!cancelled) {
          const normalized = error instanceof Error ? error : new Error(String(error));
          setStatus({ attached: false, error: normalized });
          onErrorRef.current?.(normalized);
        }
      });

    return () => {
      cancelled = true;
      controller.detachSurface(canvas);
    };
  }, [controller]);

  return {
    surfaceRef,
    canvasRef,
    focus,
    blur,
    status,
  };
}

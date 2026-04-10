import { useCallback, useEffect, useRef } from "react";
import type { CanvasCourierController } from "./types";

export function useCanvasCourierSurface({ controller }: { controller: CanvasCourierController }) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const offscreenRef = useRef<OffscreenCanvas | null>(null);

  const focus = useCallback(() => {
    canvasRef.current?.focus({ preventScroll: true });
  }, []);

  const blur = useCallback(() => {
    canvasRef.current?.blur();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = surfaceRef.current;
    if (!canvas || !container) {
      return;
    }

    offscreenRef.current ??= canvas.transferControlToOffscreen();
    controller.attachSurface({ canvas, offscreen: offscreenRef.current });

    return () => {
      controller.detachSurface(canvas);
    };
  }, [controller]);

  return {
    surfaceRef,
    canvasRef,
    focus,
    blur,
  };
}

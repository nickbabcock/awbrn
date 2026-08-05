import { useCallback, useEffect, useEffectEvent, useRef } from "react";
import type { CanvasCourierController } from "./types";
import { useGameFullscreen } from "./useGameFullscreen";

export function useCanvasCourierSurface({ controller }: { controller: CanvasCourierController }) {
  const surfaceRef = useRef<HTMLElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const offscreenRef = useRef<OffscreenCanvas | null>(null);

  const attachSurface = useEffectEvent((canvas: HTMLCanvasElement, offscreen: OffscreenCanvas) => {
    controller.attachSurface({ canvas, offscreen });
  });

  const focus = useCallback(() => {
    canvasRef.current?.focus({ preventScroll: true });
  }, []);

  const blur = useCallback(() => {
    canvasRef.current?.blur();
  }, []);

  const { enterFullscreen, exitFullscreen, isFullscreen, mode } = useGameFullscreen({
    focusSurface: focus,
    surfaceRef,
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = surfaceRef.current;
    if (!canvas || !container) {
      return;
    }

    if (offscreenRef.current === null) {
      offscreenRef.current = canvas.transferControlToOffscreen();
      attachSurface(canvas, offscreenRef.current);
    }

    // No cleanup as transferring is a one-way operation
  }, []);

  return {
    surfaceRef,
    canvasRef,
    focus,
    blur,
    enterFullscreen,
    exitFullscreen,
    fullscreenMode: mode,
    isFullscreen,
  };
}
